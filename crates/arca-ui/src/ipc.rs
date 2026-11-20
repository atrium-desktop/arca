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

/// Resolve the standard XDG runtime socket path for Arca.
pub fn socket_path() -> PathBuf {
    endpoint(None)
}

/// Resolve a socket path, optionally overriding the standard location.
///
/// Tests pass an explicit path: several tests share one process, and the
/// standard socket is a single instance-wide resource, so binding it from
/// more than one test races (the second `bind` succeeds, the first server
/// then serves the second's client).
fn endpoint(override_path: Option<&std::path::Path>) -> PathBuf {
    if let Some(path) = override_path {
        return path.to_path_buf();
    }
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
    try_forward_to(&socket_path(), path)
}

/// [`try_forward_to_existing`] against an explicit socket path.
pub fn try_forward_to(sock: &std::path::Path, path: &str) -> bool {
    if !sock.exists() {
        return false;
    }
    let Ok(mut stream) = UnixStream::connect(sock) else {
        let _ = fs::remove_file(sock);
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
    socket: PathBuf,
    running: Arc<AtomicBool>,
    /// Paths forwarded by other processes, drained by the UI each frame.
    pending: Arc<Mutex<Vec<String>>>,
}

impl IpcServer {
    /// Start the Unix domain socket listener for this instance.
    pub fn start() -> Option<Self> {
        Self::start_at(&socket_path())
    }

    /// [`IpcServer::start`] on an explicit socket path (tests).
    pub fn start_at(sock: &std::path::Path) -> Option<Self> {
        let sock = sock.to_path_buf();
        let _ = fs::remove_file(&sock);

        let listener = UnixListener::bind(&sock).ok()?;
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();
        let pending = Arc::new(Mutex::new(Vec::new()));
        let pending_worker = Arc::clone(&pending);

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
                                    if let Ok(mut queue) = pending_worker.lock() {
                                        queue.push(path.to_string());
                                    }
                                    optics_window::post_to_main_thread(|| {});
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
            socket: sock,
            running,
            pending,
        })
    }

    /// Take the paths forwarded since the last drain.
    pub fn drain(&self) -> Vec<String> {
        self.pending
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = fs::remove_file(&self.socket);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A socket path unique to one test, removed when dropped.
    struct Endpoint(PathBuf);

    impl Endpoint {
        fn new(name: &str) -> Endpoint {
            let path = std::env::temp_dir().join(format!(
                "arca-ipc-test-{name}-{}-{}.sock",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            let _ = fs::remove_file(&path);
            Endpoint(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for Endpoint {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn forwarding_roundtrip() {
        let endpoint = Endpoint::new("forward");
        let server = IpcServer::start_at(endpoint.path()).expect("server should start");

        assert!(try_forward_to(endpoint.path(), "/tmp/test-ipc-folder"));
        assert_eq!(
            server.drain(),
            vec!["/tmp/test-ipc-folder"],
            "the server must queue the forwarded path"
        );

        drop(server);
        assert!(!endpoint.path().exists(), "drop unlinks the socket");
    }

    /// A stale socket file (a crashed window) must not make forwarding
    /// believe a live instance is listening.
    #[test]
    fn forwarding_to_a_dead_socket_fails() {
        let endpoint = Endpoint::new("dead");
        fs::write(endpoint.path(), b"not a socket").unwrap();

        assert!(
            !try_forward_to(endpoint.path(), "/tmp/nowhere"),
            "a non-socket file must not accept a forward"
        );
        assert!(!endpoint.path().exists(), "the dead endpoint is cleaned up");
    }

    #[test]
    fn forwarding_without_a_socket_is_a_no_op() {
        let endpoint = Endpoint::new("absent");
        assert!(!try_forward_to(endpoint.path(), "/tmp/nowhere"));
    }

    /// Several requests in one window are all delivered.
    #[test]
    fn multiple_requests_are_queued_in_order() {
        let endpoint = Endpoint::new("batch");
        let server = IpcServer::start_at(endpoint.path()).expect("server should start");

        for path in ["/tmp/one", "/tmp/two", "/tmp/three"] {
            assert!(try_forward_to(endpoint.path(), path));
        }
        assert_eq!(server.drain(), vec!["/tmp/one", "/tmp/two", "/tmp/three"]);
        // Draining is destructive.
        assert!(server.drain().is_empty());

        drop(server);
    }

    /// The default path is the XDG runtime dir and carries the filename the
    /// CLI forwarder expects.
    #[test]
    fn default_socket_path_is_under_the_runtime_dir() {
        let path = socket_path();
        assert_eq!(path.file_name().unwrap(), "arca-ipc.sock");
        if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
            assert!(path.starts_with(runtime), "must honour XDG_RUNTIME_DIR");
        }
    }
}
