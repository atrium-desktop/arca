//! Asynchronous background IO worker for non-blocking file operations.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;

use arca_xdg::TransferAction;

use crate::ops;

/// Unique identifier for an asynchronous IO job.
pub type JobId = u64;

/// Kind of asynchronous operation.
#[derive(Clone, Debug)]
pub enum JobKind {
    Copy {
        sources: Vec<PathBuf>,
        destination: PathBuf,
    },
    Move {
        sources: Vec<PathBuf>,
        destination: PathBuf,
    },
}

/// Dynamic progress status reported by the background worker.
#[derive(Clone, Debug)]
pub struct JobProgress {
    pub job_id: JobId,
    pub bytes_copied: u64,
    pub total_bytes: u64,
    pub current_file: String,
    pub files_completed: usize,
    pub total_files: usize,
    pub is_finished: bool,
    pub error: Option<String>,
}

impl JobProgress {
    pub fn percent(&self) -> f32 {
        if self.total_bytes == 0 {
            100.0
        } else {
            ((self.bytes_copied as f64 / self.total_bytes as f64) * 100.0) as f32
        }
    }
}

/// Completion result sent when a job finishes.
#[derive(Clone, Debug)]
pub struct JobCompleted {
    pub job_id: JobId,
    pub kind: JobKind,
    pub created_destinations: Vec<PathBuf>,
    pub error: Option<String>,
}

impl JobCompleted {
    /// Number of items that reached the destination, or `None` when the job
    /// failed outright (so the caller reports the error, not a count).
    #[must_use]
    pub fn path_count(&self) -> Option<usize> {
        (self.error.is_none() && !self.created_destinations.is_empty())
            .then_some(self.created_destinations.len())
    }
}

struct JobRequest {
    id: JobId,
    kind: JobKind,
    cancel: Arc<AtomicBool>,
}

pub struct AsyncIoEngine {
    tx: Sender<JobRequest>,
    progress_rx: Receiver<JobProgress>,
    completion_rx: Receiver<JobCompleted>,
    next_id: Arc<AtomicU64>,
    active_cancel: Option<Arc<AtomicBool>>,
    /// True between `submit` and the completion event.
    busy: bool,
    pub current_progress: Option<JobProgress>,
}

impl Default for AsyncIoEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncIoEngine {
    pub fn new() -> Self {
        let (job_tx, job_rx) = channel::<JobRequest>();
        let (prog_tx, prog_rx) = channel::<JobProgress>();
        let (comp_tx, comp_rx) = channel::<JobCompleted>();

        thread::Builder::new()
            .name("arca-io-worker".into())
            .spawn(move || {
                while let Ok(req) = job_rx.recv() {
                    run_job(req, &prog_tx, &comp_tx);
                }
            })
            .expect("spawn arca-io-worker");

        Self {
            tx: job_tx,
            progress_rx: prog_rx,
            completion_rx: comp_rx,
            next_id: Arc::new(AtomicU64::new(1)),
            active_cancel: None,
            busy: false,
            current_progress: None,
        }
    }

    /// Whether a job is queued or running.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.busy
    }

    /// Submit a transfer. The action decides whether the source is consumed.
    pub fn submit(
        &mut self,
        action: TransferAction,
        sources: Vec<PathBuf>,
        destination: PathBuf,
    ) -> JobId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let cancel = Arc::new(AtomicBool::new(false));
        self.active_cancel = Some(Arc::clone(&cancel));
        self.busy = true;
        let kind = match action {
            TransferAction::Copy => JobKind::Copy {
                sources,
                destination,
            },
            TransferAction::Move => JobKind::Move {
                sources,
                destination,
            },
        };
        let _ = self.tx.send(JobRequest { id, kind, cancel });
        id
    }

    pub fn cancel_active(&mut self) {
        if let Some(cancel) = &self.active_cancel {
            cancel.store(true, Ordering::SeqCst);
        }
    }

    /// Poll for progress updates and completion events without blocking.
    pub fn poll(&mut self) -> Option<JobCompleted> {
        while let Ok(prog) = self.progress_rx.try_recv() {
            if prog.is_finished {
                self.current_progress = None;
                self.busy = false;
            } else {
                self.current_progress = Some(prog);
            }
        }
        self.completion_rx.try_recv().ok()
    }
}

fn calculate_total_bytes(paths: &[PathBuf]) -> (u64, usize) {
    let mut total_bytes = 0u64;
    let mut total_files = 0usize;

    for p in paths {
        count_path(p, &mut total_bytes, &mut total_files);
    }
    (total_bytes, total_files)
}

fn count_path(p: &Path, bytes: &mut u64, files: &mut usize) {
    // `symlink_metadata` keeps symlinked directories out of the walk: a link
    // pointing at an ancestor would otherwise recurse without bound.
    if let Ok(meta) = fs::symlink_metadata(p) {
        if meta.is_dir() {
            if let Ok(read) = fs::read_dir(p) {
                for entry in read.flatten() {
                    count_path(&entry.path(), bytes, files);
                }
            }
        } else {
            *bytes += meta.len();
            *files += 1;
        }
    }
}

fn run_job(req: JobRequest, prog_tx: &Sender<JobProgress>, comp_tx: &Sender<JobCompleted>) {
    let (sources, dest, is_move) = match &req.kind {
        JobKind::Copy {
            sources,
            destination,
        } => (sources, destination, false),
        JobKind::Move {
            sources,
            destination,
        } => (sources, destination, true),
    };

    let (total_bytes, total_files) = calculate_total_bytes(sources);
    let mut bytes_copied = 0u64;
    let mut files_completed = 0usize;
    let mut created_destinations = Vec::new();
    let mut job_error = None;

    for src in sources {
        if req.cancel.load(Ordering::Relaxed) {
            job_error = Some("Operation cancelled".to_string());
            break;
        }

        let name = match src.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };

        let res = if is_move {
            // `move_into` enforces the self-containment guard itself.
            match ops::move_into(src, dest) {
                Ok(actual_dst) => {
                    bytes_copied += fs::metadata(&actual_dst).map(|m| m.len()).unwrap_or(0);
                    files_completed += 1;
                    Ok(actual_dst)
                }
                Err(e) => Err(e),
            }
        } else {
            // The progressive copy below is a hot path that cannot report
            // progress through `ops::copy_into`, so it re-implements the walk.
            // It must therefore re-apply the same preconditions: without the
            // guard, a large multi-selection could be copied into its own
            // descendant and recurse until ENAMETOOLONG — the exact failure
            // the synchronous path refuses up front.
            match ops::ensure_transfer_allowed(src, dest) {
                Err(e) => Err(e),
                Ok(()) => {
                    let dst = ops::unique_destination(dest, &name);
                    let mut ctx = ProgressContext {
                        job_id: req.id,
                        cancel: req.cancel.as_ref(),
                        total_bytes,
                        total_files,
                        bytes_copied: &mut bytes_copied,
                        files_completed: &mut files_completed,
                        prog_tx,
                    };
                    copy_with_progress(src, &dst, &mut ctx).map(|()| dst)
                }
            }
        };

        match res {
            Ok(created) => created_destinations.push(created),
            Err(e) => {
                job_error = Some(e.to_string());
                break;
            }
        }
    }

    let _ = prog_tx.send(JobProgress {
        job_id: req.id,
        bytes_copied,
        total_bytes,
        current_file: String::new(),
        files_completed,
        total_files,
        is_finished: true,
        error: job_error.clone(),
    });

    let _ = comp_tx.send(JobCompleted {
        job_id: req.id,
        kind: req.kind,
        created_destinations,
        error: job_error,
    });
}

struct ProgressContext<'a> {
    job_id: JobId,
    cancel: &'a AtomicBool,
    total_bytes: u64,
    total_files: usize,
    bytes_copied: &'a mut u64,
    files_completed: &'a mut usize,
    prog_tx: &'a Sender<JobProgress>,
}

fn copy_with_progress(src: &Path, dst: &Path, ctx: &mut ProgressContext<'_>) -> io::Result<()> {
    if ctx.cancel.load(Ordering::Relaxed) {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "Cancelled"));
    }

    let meta = fs::symlink_metadata(src)?;
    if meta.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let child_dst = dst.join(entry.file_name());
            copy_with_progress(&entry.path(), &child_dst, ctx)?;
        }
    } else if meta.is_symlink() {
        let target = fs::read_link(src)?;
        std::os::unix::fs::symlink(target, dst)?;
        *ctx.files_completed += 1;
    } else {
        let mut r = File::open(src)?;
        let mut w = File::create(dst)?;
        let mut buf = [0u8; 64 * 1024];

        let file_name = src
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        loop {
            if ctx.cancel.load(Ordering::Relaxed) {
                let _ = fs::remove_file(dst);
                return Err(io::Error::new(io::ErrorKind::Interrupted, "Cancelled"));
            }

            let n = r.read(&mut buf)?;
            if n == 0 {
                break;
            }
            w.write_all(&buf[..n])?;
            *ctx.bytes_copied += n as u64;

            let _ = ctx.prog_tx.send(JobProgress {
                job_id: ctx.job_id,
                bytes_copied: *ctx.bytes_copied,
                total_bytes: ctx.total_bytes,
                current_file: file_name.clone(),
                files_completed: *ctx.files_completed,
                total_files: ctx.total_files,
                is_finished: false,
                error: None,
            });
        }
        *ctx.files_completed += 1;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn async_io_copy_and_progress() {
        let temp_dir = std::env::temp_dir().join(format!(
            "arca-async-io-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();

        let src = temp_dir.join("source.dat");
        fs::write(&src, vec![0x42; 256 * 1024]).unwrap(); // 256 KiB file

        let dst_dir = temp_dir.join("dest_folder");
        fs::create_dir_all(&dst_dir).unwrap();

        let mut engine = AsyncIoEngine::new();
        assert!(!engine.is_busy());
        let job_id = engine.submit(TransferAction::Copy, vec![src.clone()], dst_dir.clone());
        assert!(job_id > 0);
        assert!(engine.is_busy(), "a submitted job marks the engine busy");

        let mut completed = None;
        for _ in 0..200 {
            if let Some(comp) = engine.poll() {
                completed = Some(comp);
                break;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }

        let comp = completed.expect("job should complete");
        assert_eq!(comp.job_id, job_id);
        assert!(comp.error.is_none());
        assert_eq!(comp.path_count(), Some(1));
        assert!(dst_dir.join("source.dat").exists());
        assert_eq!(
            fs::metadata(dst_dir.join("source.dat")).unwrap().len(),
            256 * 1024
        );
        assert!(!engine.is_busy(), "completion clears the busy flag");

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    /// The progressive copy path re-implements the walk, so it must re-apply
    /// the self-containment guard: a large multi-selection copied into its
    /// own descendant must be refused, not recurse to ENAMETOOLONG.
    #[test]
    fn async_copy_refuses_a_directory_into_its_own_descendant() {
        let temp_dir = std::env::temp_dir().join(format!(
            "arca-async-self-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let outer = temp_dir.join("outer");
        let inner = outer.join("inner");
        fs::create_dir_all(&inner).unwrap();
        fs::write(outer.join("f.txt"), b"x").unwrap();

        let mut engine = AsyncIoEngine::new();
        engine.submit(TransferAction::Copy, vec![outer.clone()], inner.clone());

        let mut finished = None;
        for _ in 0..300 {
            if let Some(comp) = engine.poll() {
                finished = Some(comp);
                break;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }
        let comp = finished.expect("the job must finish");
        let error = comp.error.as_deref().unwrap_or_default();
        assert!(
            error.contains("inside itself"),
            "the worker must refuse self-containment, got {error:?}"
        );
        assert!(comp.created_destinations.is_empty());
        assert_eq!(
            fs::read_dir(&inner).unwrap().count(),
            0,
            "nothing may be created inside the descendant"
        );

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    /// A cancelled job stops promptly and reports the cancellation, and the
    /// partially written destination is not left behind.
    #[test]
    fn async_io_cancellation_stops_the_job() {
        let temp_dir = std::env::temp_dir().join(format!(
            "arca-async-cancel-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).unwrap();
        let src = temp_dir.join("big.dat");
        // 24 MiB so the copy spans many 64 KiB chunks.
        fs::write(&src, vec![0x5a; 24 * 1024 * 1024]).unwrap();
        let dst_dir = temp_dir.join("out");
        fs::create_dir_all(&dst_dir).unwrap();

        let mut engine = AsyncIoEngine::new();
        engine.submit(TransferAction::Copy, vec![src], dst_dir.clone());
        engine.cancel_active();

        let mut finished = None;
        for _ in 0..300 {
            if let Some(comp) = engine.poll() {
                finished = Some(comp);
                break;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }
        let comp = finished.expect("a cancelled job still reports completion");
        let error = comp.error.as_deref().unwrap_or_default();
        assert!(
            error.contains("ancelled"),
            "cancellation must be reported, got {error:?}"
        );

        fs::remove_dir_all(&temp_dir).unwrap();
    }
}
