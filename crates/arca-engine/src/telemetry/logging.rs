//! Log management: the single concrete `log` backend for every Arca crate.
//!
//! Records go to two sinks with independent filters:
//!
//! - a size-rotated file, `<state>/logs/arca.log` (+ `arca.log.1` …
//!   `arca.log.N`), filtered by `ARCA_LOG` (default `info`);
//! - stderr, filtered by `ARCA_LOG_STDERR` (default `warn`).
//!
//! Nothing is ever written to stdout: the prompter speaks its protocol there.
//!
//! Filter specs use the familiar `env_logger` shape: a default level and
//! optional `target=level` overrides, comma separated, longest-prefix wins —
//! `info,arca_engine::watch=debug,arca::apm=off`.
//!
//! Each line is `TIMESTAMP LEVEL [thread] target: message`, one record per
//! line (embedded newlines are indented), so `grep`/`awk` work on it.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use log::{LevelFilter, Log, Metadata, Record};

use super::metrics;

/// Rotate once the live file reaches this size.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// Rotated generations kept beside the live file.
pub const KEEP_ROTATED: usize = 4;
/// File name of the live log.
pub const FILE_NAME: &str = "arca.log";

/// A parsed filter spec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filter {
    default: LevelFilter,
    directives: Vec<(String, LevelFilter)>,
}

fn parse_level(text: &str) -> Option<LevelFilter> {
    text.trim().parse().ok()
}

impl Filter {
    /// Parse `spec`. Unparseable parts are ignored; an empty spec means `info`.
    #[must_use]
    pub fn parse(spec: &str) -> Filter {
        let mut filter = Filter {
            default: LevelFilter::Info,
            directives: Vec::new(),
        };
        for part in spec.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            match part.split_once('=') {
                Some((target, level)) => {
                    if let Some(level) = parse_level(level) {
                        filter.directives.push((target.trim().to_string(), level));
                    }
                }
                None => {
                    if let Some(level) = parse_level(part) {
                        filter.default = level;
                    }
                }
            }
        }
        // Longest prefix first, so lookup can stop at the first match.
        filter
            .directives
            .sort_by_key(|(target, _)| std::cmp::Reverse(target.len()));
        filter
    }

    /// The level enabled for `target`.
    #[must_use]
    pub fn level_for(&self, target: &str) -> LevelFilter {
        self.directives
            .iter()
            .find(|(prefix, _)| {
                target == prefix
                    || (target.starts_with(prefix.as_str())
                        && target[prefix.len()..].starts_with("::"))
            })
            .map_or(self.default, |(_, level)| *level)
    }

    /// The most verbose level any target can reach.
    #[must_use]
    pub fn max_level(&self) -> LevelFilter {
        self.directives
            .iter()
            .map(|(_, level)| *level)
            .fold(self.default, Ord::max)
    }

    #[must_use]
    pub fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.level_for(metadata.target())
    }
}

/// Render one record as a log line (without the trailing newline).
#[must_use]
pub fn format_line(record: &Record<'_>, unix_ms: u64, thread: &str) -> String {
    let message = record.args().to_string().replace('\n', "\n    ");
    format!(
        "{} {:<5} [{}] {}: {}",
        super::format_utc_ms(unix_ms),
        record.level(),
        thread,
        record.target(),
        message
    )
}

/// `line` with its record terminator, ready for a single write.
fn terminated(line: &str) -> String {
    let mut record = String::with_capacity(line.len() + 1);
    record.push_str(line);
    record.push('\n');
    record
}

/// Append-only log file that rotates by size.
pub struct RotatingFile {
    path: PathBuf,
    file: File,
    written: u64,
    max_bytes: u64,
    keep: usize,
}

impl RotatingFile {
    /// Open (creating `dir`) `dir/name` for appending.
    pub fn open(dir: &Path, name: &str, max_bytes: u64, keep: usize) -> io::Result<RotatingFile> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(name);
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let written = file.metadata()?.len();
        Ok(RotatingFile {
            path,
            file,
            written,
            max_bytes,
            keep,
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn rotated(&self, generation: usize) -> PathBuf {
        let mut name = self.path.as_os_str().to_owned();
        name.push(format!(".{generation}"));
        PathBuf::from(name)
    }

    /// Shift `name.N-1 → name.N`, …, `name → name.1`, then reopen.
    fn rotate(&mut self) -> io::Result<()> {
        self.file.flush()?;
        if self.keep == 0 {
            self.file = File::create(&self.path)?;
        } else {
            let _ = std::fs::remove_file(self.rotated(self.keep));
            for generation in (1..self.keep).rev() {
                let from = self.rotated(generation);
                if from.exists() {
                    std::fs::rename(&from, self.rotated(generation + 1))?;
                }
            }
            std::fs::rename(&self.path, self.rotated(1))?;
            self.file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?;
        }
        self.written = 0;
        Ok(())
    }

    /// Append `line` plus a newline, rotating first if it would overflow.
    ///
    /// The record goes out in one `write(2)`: window, prompters, and
    /// chooser share this file, and `O_APPEND` keeps only a single write
    /// whole — a separate newline write let concurrent processes splice
    /// their records onto one line.
    pub fn write_line(&mut self, line: &str) -> io::Result<()> {
        let record = terminated(line);
        let bytes = record.len() as u64;
        if self.written > 0 && self.written + bytes > self.max_bytes {
            self.rotate()?;
        }
        self.file.write_all(record.as_bytes())?;
        self.written += bytes;
        Ok(())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// The process logger.
pub struct Logger {
    file_filter: Filter,
    stderr_filter: Filter,
    file: Option<Mutex<RotatingFile>>,
}

impl Logger {
    /// Build a logger; when `log_dir` cannot be opened the file sink is
    /// dropped (reported on stderr) and logging continues on stderr.
    /// Returns the logger and the live log path, if any.
    #[must_use]
    pub fn build(
        file_filter: Filter,
        stderr_filter: Filter,
        log_dir: Option<&Path>,
    ) -> (Logger, Option<PathBuf>) {
        let file = log_dir.and_then(|dir| {
            match RotatingFile::open(dir, FILE_NAME, MAX_FILE_BYTES, KEEP_ROTATED) {
                Ok(file) => Some(file),
                Err(error) => {
                    eprintln!("arca: cannot open log directory {}: {error}", dir.display());
                    None
                }
            }
        });
        let path = file.as_ref().map(|f| f.path().to_path_buf());
        (
            Logger {
                file_filter,
                stderr_filter,
                file: file.map(Mutex::new),
            },
            path,
        )
    }

    /// The most verbose level either sink accepts.
    #[must_use]
    pub fn max_level(&self) -> LevelFilter {
        let file = if self.file.is_some() {
            self.file_filter.max_level()
        } else {
            LevelFilter::Off
        };
        Ord::max(file, self.stderr_filter.max_level())
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        (self.file.is_some() && self.file_filter.enabled(metadata))
            || self.stderr_filter.enabled(metadata)
    }

    fn log(&self, record: &Record<'_>) {
        let metadata = record.metadata();
        let to_file = self.file.is_some() && self.file_filter.enabled(metadata);
        let to_stderr = self.stderr_filter.enabled(metadata);
        if !to_file && !to_stderr {
            return;
        }
        match record.level() {
            log::Level::Error => metrics::counter("log.errors", 1),
            log::Level::Warn => metrics::counter("log.warnings", 1),
            _ => {}
        }
        let thread = std::thread::current();
        let line = format_line(
            record,
            super::unix_ms(SystemTime::now()),
            thread.name().unwrap_or("unnamed"),
        );
        if to_file && let Some(file) = &self.file {
            let mut file = file
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if file.write_line(&line).is_err() {
                metrics::counter("log.write_failures", 1);
            }
            // Warnings and errors are what a crash report needs; make
            // them durable immediately rather than on buffer pressure.
            if record.level() <= log::Level::Warn {
                let _ = file.flush();
            }
        }
        if to_stderr {
            // One write, for the same reason as the file sink.
            let _ = io::stderr().lock().write_all(terminated(&line).as_bytes());
        }
    }

    fn flush(&self) {
        if let Some(file) = &self.file {
            let _ = file
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .flush();
        }
    }
}

/// Install `logger` as the process logger. Returns false if one was
/// already installed (the first one stays).
pub fn install(logger: Logger) -> bool {
    let max = logger.max_level();
    match log::set_boxed_logger(Box::new(logger)) {
        Ok(()) => {
            log::set_max_level(max);
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::fs::TestDir;

    #[test]
    fn filter_specs_parse_defaults_and_overrides() {
        let filter = Filter::parse("warn,arca_engine=info,arca_engine::watch=trace,bogus=nope");
        assert_eq!(filter.level_for("arca_ui::app"), LevelFilter::Warn);
        assert_eq!(filter.level_for("arca_engine::state"), LevelFilter::Info);
        assert_eq!(filter.level_for("arca_engine::watch"), LevelFilter::Trace);
        assert_eq!(
            filter.level_for("arca_engine::watch::linux"),
            LevelFilter::Trace
        );
        // A prefix must end on a module boundary.
        assert_eq!(filter.level_for("arca_engineering"), LevelFilter::Warn);
        assert_eq!(filter.max_level(), LevelFilter::Trace);
        assert_eq!(Filter::parse("").level_for("x"), LevelFilter::Info);
        assert_eq!(Filter::parse("off").max_level(), LevelFilter::Off);
    }

    #[test]
    fn lines_carry_timestamp_level_thread_target_and_indent_newlines() {
        let line = format_line(
            &Record::builder()
                .level(log::Level::Warn)
                .target("arca::alert")
                .args(format_args!("first\nsecond"))
                .build(),
            0,
            "main",
        );
        assert_eq!(
            line,
            "1970-01-01T00:00:00.000Z WARN  [main] arca::alert: first\n    second"
        );
    }

    #[test]
    fn the_file_rotates_by_size_and_keeps_bounded_generations() {
        let dir = TestDir::new("arca-log-rotate");
        let mut file = RotatingFile::open(dir.path(), "t.log", 100, 2).unwrap();
        let line = "x".repeat(59); // 60 bytes with newline: one line per file
        for generation in 0..5 {
            file.write_line(&format!("{generation}{line}")).unwrap();
        }
        file.flush().unwrap();
        let read = |name: &str| std::fs::read_to_string(dir.path().join(name)).unwrap();
        assert!(read("t.log").starts_with('4'));
        assert!(read("t.log.1").starts_with('3'));
        assert!(read("t.log.2").starts_with('2'));
        assert!(
            !dir.path().join("t.log.3").exists(),
            "only `keep` generations"
        );
    }

    /// Window, prompters, and chooser each hold their own append handle on
    /// one file; their records must never splice together (they did when
    /// the newline was a second write).
    #[test]
    fn concurrent_writers_on_separate_handles_keep_records_whole() {
        let dir = TestDir::new("arca-log-concurrent");
        let writers: Vec<_> = (0..8)
            .map(|writer| {
                let path = dir.path().to_path_buf();
                std::thread::spawn(move || {
                    let mut file = RotatingFile::open(&path, "c.log", u64::MAX, 1).unwrap();
                    for i in 0..500 {
                        file.write_line(&format!("writer={writer} record={i} end"))
                            .unwrap();
                    }
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        let text = std::fs::read_to_string(dir.path().join("c.log")).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 8 * 500);
        for line in lines {
            assert!(
                line.starts_with("writer=")
                    && line.ends_with(" end")
                    && line.matches("end").count() == 1,
                "spliced record: {line:?}"
            );
        }
    }

    #[test]
    fn reopening_appends_and_counts_existing_bytes() {
        let dir = TestDir::new("arca-log-append");
        {
            let mut file = RotatingFile::open(dir.path(), "a.log", 1024, 1).unwrap();
            file.write_line("one").unwrap();
        }
        let mut file = RotatingFile::open(dir.path(), "a.log", 1024, 1).unwrap();
        file.write_line("two").unwrap();
        file.flush().unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.log")).unwrap(),
            "one\ntwo\n"
        );
    }

    #[test]
    fn logger_routes_by_sink_filter_and_counts_errors() {
        let dir = TestDir::new("arca-logger");
        let (logger, path) = Logger::build(
            Filter::parse("info,noisy=off"),
            Filter::parse("off"),
            Some(dir.path()),
        );
        let path = path.expect("file sink opened");
        let errors_before = metrics::global().counter_value("log.errors");
        let emit = |level, target: &str, message: &str| {
            logger.log(
                &Record::builder()
                    .level(level)
                    .target(target)
                    .args(format_args!("{message}"))
                    .build(),
            );
        };
        emit(log::Level::Info, "arca_engine::state", "kept");
        emit(log::Level::Debug, "arca_engine::state", "below level");
        emit(log::Level::Error, "noisy", "filtered target");
        emit(log::Level::Error, "arca", "boom");
        logger.flush();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("arca_engine::state: kept"), "{text}");
        assert!(
            text.contains("ERROR [") && text.contains("arca: boom"),
            "{text}"
        );
        assert!(!text.contains("below level") && !text.contains("filtered target"));
        assert!(metrics::global().counter_value("log.errors") > errors_before);
        assert_eq!(logger.max_level(), LevelFilter::Info);
    }

    #[test]
    fn an_unwritable_log_directory_degrades_to_stderr_only() {
        let dir = TestDir::new("arca-log-blocked");
        let blocker = dir.path().join("not-a-dir");
        std::fs::write(&blocker, b"").unwrap();
        let (logger, path) =
            Logger::build(Filter::parse("info"), Filter::parse("warn"), Some(&blocker));
        assert!(path.is_none());
        assert_eq!(logger.max_level(), LevelFilter::Warn);
    }
}
