//! Single-Instance IPC socket listener and command client.
//!
//! Enables command line invocations (e.g. `arca /path/to/folder`) to open
//! a new tab in an already running Arca instance instead of spawning duplicate windows.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

static PENDING_OPEN_PATHS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Resolve the standard XDG runtime socket path for Arca.
pub fn socket_path() -> PathBuf {
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime).join("arca-ipc.sock")
    } else {
        let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
        std::env::temp_dir().join(format!("arca-ipc-{user}.sock"))
    }
}

/// Attempt to forward an open request to an already running instance.
/// Returns `true` if the request was accepted (caller may exit immediately).
pub fn try_forward_to_existing(path: &str) -> bool {
    let sock = socket_path();
    if !sock.exists() {
        return false;
    }
    let Ok(mut stream) = UnixStream::connect(&sock) else {
        let _ = fs::remove_file(&sock);
        return false;
    };

    let abs_path = if path.starts_with('/') || path.starts_with('~') {
        path.to_string()
    } else {
        std::env::current_dir()
            .map(|c| c.join(path).to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.to_string())
    };

    let msg = format!("OPEN\t{abs_path}\n");
    if stream.write_all(msg.as_bytes()).is_err() {
        return false;
    }

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response).is_ok() && response.starts_with("OK")
}

/// Background IPC server instance for the active Arca window.
pub struct IpcServer {
    socket_path: PathBuf,
    running: Arc<AtomicBool>,
}

impl IpcServer {
    /// Start the Unix domain socket listener for this instance.
    pub fn start() -> Option<Self> {
        let sock = socket_path();
        let _ = fs::remove_file(&sock);

        let listener = UnixListener::bind(&sock).ok()?;
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        thread::Builder::new()
            .name("arca-ipc-server".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    if !running_clone.load(Ordering::Relaxed) {
                        break;
                    }
                    if let Ok(stream) = stream {
                        if let Ok(stream_clone) = stream.try_clone() {
                            let mut reader = BufReader::new(stream_clone);
                            let mut line = String::new();
                            if reader.read_line(&mut line).is_ok() {
                                let trimmed = line.trim();
                                if let Some(path) = trimmed.strip_prefix("OPEN\t") {
                                    if let Ok(mut queue) = PENDING_OPEN_PATHS.lock() {
                                        queue.push(path.to_string());
                                    }
                                    iris::post_to_main_thread(|| {});
                                    let mut writer = stream;
                                    let _ = writer.write_all(b"OK\n");
                                }
                            }
                        }
                    }
                }
            })
            .ok()?;

        Some(Self {
            socket_path: sock,
            running,
        })
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = fs::remove_file(&self.socket_path);
    }
}

/// Drain pending external open requests received via IPC.
pub fn drain_pending_open_requests() -> Vec<String> {
    if let Ok(mut queue) = PENDING_OPEN_PATHS.lock() {
        std::mem::take(&mut *queue)
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_server_forwarding_roundtrip() {
        let server = IpcServer::start().expect("server should start");

        let forwarded = try_forward_to_existing("/tmp/test-ipc-folder");
        assert!(forwarded, "forward should succeed");

        let drained = drain_pending_open_requests();
        assert_eq!(drained, vec!["/tmp/test-ipc-folder"]);

        drop(server);
        assert!(!socket_path().exists(), "socket should be cleaned up on drop");
    }
}
