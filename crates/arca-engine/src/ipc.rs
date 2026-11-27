//! Hardened Single-Instance IPC (`arca-ipc-v2`, ADR-0023).
//!
//! Replaces the legacy line-oriented `"OPEN\t<path>\n"` text protocol with
//! length-prefixed (`u32_be`, capped at `64 KiB`) strict JSON frames over a
//! private `0600` Unix domain socket verified via Linux `SO_PEERCRED`:
//! - Preserves arbitrary native Unix paths (spaces, trailing whitespace, tabs,
//!   newlines, non-UTF-8 bytes) via [`BytePath`].
//! - Carries optional `XDG_ACTIVATION_TOKEN` so an existing Wayland window can
//!   raise itself properly when opened from another application.
//! - Isolates each peer onto a bounded, timed worker so a slow or stalled
//!   client can never deadlock the listener.
//! - Never falls back to world-writable `/tmp`: resolves
//!   `$XDG_RUNTIME_DIR/arca/ipc.sock` or `/run/user/<euid>/arca/ipc.sock`.

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::chooser::BytePath;

/// Wire version of the single-instance IPC envelope.
pub const IPC_PROTOCOL_VERSION: u32 = 2;

/// Maximum serialized payload size accepted on a single IPC frame (64 KiB).
pub const MAX_IPC_FRAME_BYTES: usize = 64 * 1024;

/// Maximum time allowed for reading or writing a single IPC frame.
pub const IPC_IO_TIMEOUT: Duration = Duration::from_secs(2);

/// Maximum concurrent in-flight peer handlers.
const MAX_ACTIVE_PEERS: usize = 16;

/// Maximum queued forwarded requests waiting for the UI thread to drain.
const MAX_PENDING_FORWARDS: usize = 256;

/// Subpath under `$XDG_RUNTIME_DIR` (or `/run/user/<euid>`).
pub const SOCKET_SUBPATH: &str = "arca/ipc.sock";

/// One forwarded open request drained by the active UI window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardedOpen {
    pub path: BytePath,
    pub activation_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct IpcRequest {
    version: u32,
    command: IpcCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum IpcCommand {
    Open {
        path: BytePath,
        #[serde(default)]
        activation_token: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum IpcResponse {
    Ok,
    Error { message: String },
}

/// Resolve the standard private runtime socket path for Arca:
/// `$XDG_RUNTIME_DIR/arca/ipc.sock`, or `/run/user/<euid>/arca/ipc.sock`.
/// Never falls back to `/tmp` (ADR-0023).
pub fn socket_path() -> PathBuf {
    if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR")
        && !runtime.is_empty()
    {
        return PathBuf::from(runtime).join(SOCKET_SUBPATH);
    }
    let uid = current_euid();
    PathBuf::from(format!("/run/user/{uid}/{SOCKET_SUBPATH}"))
}

/// Attempt to forward `path` (capturing `XDG_ACTIVATION_TOKEN` if present) to
/// an already-running Arca instance at the default socket path.
pub fn try_forward_to_existing(path: &str) -> bool {
    try_forward_to(&socket_path(), path)
}

/// Forward `path` (capturing `XDG_ACTIVATION_TOKEN` from the environment) to
/// an explicit socket path.
pub fn try_forward_to(sock: &Path, path: &str) -> bool {
    let token = std::env::var("XDG_ACTIVATION_TOKEN")
        .ok()
        .filter(|t| !t.is_empty());
    try_forward_bytes(sock, Path::new(path), token)
}

/// Forward an arbitrary native `Path` and explicit Wayland activation token to
/// an explicit socket path.
pub fn try_forward_bytes(sock: &Path, path: &Path, activation_token: Option<String>) -> bool {
    if !sock.exists() {
        return false;
    }
    let Ok(mut stream) = UnixStream::connect(sock) else {
        let _ = fs::remove_file(sock);
        return false;
    };
    let _ = stream.set_read_timeout(Some(IPC_IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IPC_IO_TIMEOUT));

    let resolved = if path.is_absolute() || path.to_string_lossy().starts_with('~') {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };

    let req = IpcRequest {
        version: IPC_PROTOCOL_VERSION,
        command: IpcCommand::Open {
            path: BytePath::from_path(resolved),
            activation_token,
        },
    };
    if write_frame(&mut stream, &req).is_err() {
        return false;
    }
    matches!(
        read_frame::<_, IpcResponse>(&mut stream),
        Ok(IpcResponse::Ok)
    )
}

/// Background single-instance IPC server.
pub struct IpcServer {
    socket: PathBuf,
    running: Arc<AtomicBool>,
    pending: Arc<Mutex<Vec<ForwardedOpen>>>,
}

impl IpcServer {
    /// Bind the default per-user runtime socket (`0700` parent, `0600` socket).
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> Option<Self> {
        Self::start_at(&socket_path(), wake)
    }

    /// Bind an explicit socket path (tests and custom layouts).
    pub fn start_at(sock: &Path, wake: impl Fn() + Send + Sync + 'static) -> Option<Self> {
        let sock = sock.to_path_buf();
        if let Some(parent) = sock.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent).ok()?;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
        let _ = fs::remove_file(&sock);

        let listener = UnixListener::bind(&sock).ok()?;
        let _ = fs::set_permissions(&sock, fs::Permissions::from_mode(0o600));

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let pending = Arc::new(Mutex::new(Vec::new()));
        let pending_worker = Arc::clone(&pending);
        let wake = Arc::new(wake);
        let active_peers = Arc::new(AtomicUsize::new(0));

        thread::Builder::new()
            .name("arca-ipc-server".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    if !running_clone.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(mut stream) = stream else {
                        continue;
                    };
                    if active_peers.fetch_add(1, Ordering::AcqRel) >= MAX_ACTIVE_PEERS {
                        active_peers.fetch_sub(1, Ordering::AcqRel);
                        let _ = stream.set_write_timeout(Some(Duration::from_millis(250)));
                        let _ = write_frame(
                            &mut stream,
                            &IpcResponse::Error {
                                message: "too many concurrent IPC peers".into(),
                            },
                        );
                        continue;
                    }
                    let pending = Arc::clone(&pending_worker);
                    let wake = Arc::clone(&wake);
                    let peers = Arc::clone(&active_peers);
                    let _ = thread::Builder::new()
                        .name("arca-ipc-peer".into())
                        .spawn(move || {
                            struct PeerGuard(Arc<AtomicUsize>);
                            impl Drop for PeerGuard {
                                fn drop(&mut self) {
                                    self.0.fetch_sub(1, Ordering::AcqRel);
                                }
                            }
                            let _guard = PeerGuard(peers);
                            handle_peer(stream, &pending, &*wake);
                        });
                }
            })
            .ok()?;

        Some(Self {
            socket: sock,
            running,
            pending,
        })
    }

    /// Take all forwarded requests received since the last drain.
    pub fn drain(&self) -> Vec<ForwardedOpen> {
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

fn handle_peer(
    mut stream: UnixStream,
    pending: &Mutex<Vec<ForwardedOpen>>,
    wake: &(dyn Fn() + Send + Sync),
) {
    let _ = stream.set_read_timeout(Some(IPC_IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IPC_IO_TIMEOUT));

    if let Err(reason) = verify_peer_same_uid(&stream) {
        let _ = write_frame(&mut stream, &IpcResponse::Error { message: reason });
        return;
    }

    let req: IpcRequest = match read_frame(&mut stream) {
        Ok(req) => req,
        Err(error) => {
            let _ = write_frame(
                &mut stream,
                &IpcResponse::Error {
                    message: error.to_string(),
                },
            );
            return;
        }
    };

    if req.version != IPC_PROTOCOL_VERSION {
        let _ = write_frame(
            &mut stream,
            &IpcResponse::Error {
                message: format!(
                    "unsupported IPC protocol version {} (expected {IPC_PROTOCOL_VERSION})",
                    req.version
                ),
            },
        );
        return;
    }

    match req.command {
        IpcCommand::Open {
            path,
            activation_token,
        } => {
            if path.is_empty() || path.0.contains(&0) {
                let _ = write_frame(
                    &mut stream,
                    &IpcResponse::Error {
                        message: "forwarded path must be non-empty and NUL-free".into(),
                    },
                );
                return;
            }
            if let Ok(mut queue) = pending.lock() {
                if queue.len() >= MAX_PENDING_FORWARDS {
                    let _ = write_frame(
                        &mut stream,
                        &IpcResponse::Error {
                            message: "forward queue full".into(),
                        },
                    );
                    return;
                }
                queue.push(ForwardedOpen {
                    path,
                    activation_token,
                });
            }
            wake();
            let _ = write_frame(&mut stream, &IpcResponse::Ok);
        }
    }
}

fn write_frame<W: Write, T: serde::Serialize>(writer: &mut W, value: &T) -> io::Result<()> {
    let payload =
        serde_json::to_vec(value).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    if payload.len() > MAX_IPC_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "IPC frame exceeds 64 KiB limit",
        ));
    }
    let len = payload.len() as u32;
    writer.write_all(&len.to_be_bytes())?;
    writer.write_all(&payload)?;
    writer.flush()
}

fn read_frame<R: Read, T: serde::de::DeserializeOwned>(reader: &mut R) -> io::Result<T> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 || len > MAX_IPC_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("IPC frame length {len} outside 1..={MAX_IPC_FRAME_BYTES}"),
        ));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(unix)]
fn current_euid() -> u32 {
    unsafe extern "C" {
        /// `geteuid(2)` is thread-safe and has no preconditions.
        safe fn geteuid() -> u32;
    }
    geteuid()
}

#[cfg(not(unix))]
fn current_euid() -> u32 {
    0
}

#[cfg(all(
    target_os = "linux",
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "x86",
        target_arch = "arm",
        target_arch = "riscv64"
    )
))]
fn verify_peer_same_uid(stream: &UnixStream) -> Result<(), String> {
    use std::os::unix::io::AsRawFd;

    #[repr(C)]
    struct UCred {
        pid: i32,
        uid: u32,
        gid: u32,
    }

    const SOL_SOCKET: i32 = 1;
    const SO_PEERCRED: i32 = 17;

    unsafe extern "C" {
        fn getsockopt(
            sockfd: i32,
            level: i32,
            optname: i32,
            optval: *mut std::ffi::c_void,
            optlen: *mut u32,
        ) -> i32;
    }

    let mut cred = UCred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<UCred>() as u32;
    // SAFETY: `cred` is properly aligned and sized for `SO_PEERCRED` on Linux.
    let rc = unsafe {
        getsockopt(
            stream.as_raw_fd(),
            SOL_SOCKET,
            SO_PEERCRED,
            (&mut cred as *mut UCred).cast(),
            &mut len,
        )
    };
    if rc != 0 {
        return Err("failed to read SO_PEERCRED".into());
    }
    let my_uid = current_euid();
    if cred.uid != my_uid {
        return Err(format!(
            "peer UID {} does not match owner UID {}",
            cred.uid, my_uid
        ));
    }
    Ok(())
}

#[cfg(not(all(
    target_os = "linux",
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "x86",
        target_arch = "arm",
        target_arch = "riscv64"
    )
)))]
fn verify_peer_same_uid(_stream: &UnixStream) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    struct Endpoint(PathBuf);

    impl Endpoint {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "arca-ipc-v2-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&dir).unwrap();
            Self(dir.join("ipc.sock"))
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Endpoint {
        fn drop(&mut self) {
            if let Some(parent) = self.0.parent() {
                let _ = fs::remove_dir_all(parent);
            }
        }
    }

    #[test]
    fn lossless_roundtrip_preserves_special_chars_non_utf8_and_activation_token() {
        let ep = Endpoint::new("lossless");
        let wakes = Arc::new(AtomicUsize::new(0));
        let wakes_clone = Arc::clone(&wakes);
        let server = IpcServer::start_at(ep.path(), move || {
            wakes_clone.fetch_add(1, Ordering::Relaxed);
        })
        .expect("server binds");

        // Verify 0600 permissions on the bound socket.
        let mode = fs::metadata(ep.path()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "socket must be 0600");

        // Path with trailing space, tab, newline, and raw non-UTF-8 byte 0xFF.
        let tricky = PathBuf::from(OsString::from_vec(b"/tmp/dir\twith\nspace \xff".to_vec()));
        assert!(try_forward_bytes(
            ep.path(),
            &tricky,
            Some("wayland-token-42".into())
        ));

        let drained = server.drain();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].path.to_path_buf(), tricky);
        assert_eq!(
            drained[0].activation_token.as_deref(),
            Some("wayland-token-42")
        );
        assert_eq!(wakes.load(Ordering::Relaxed), 1);

        drop(server);
        assert!(!ep.path().exists());
    }

    #[test]
    fn slow_peer_does_not_block_healthy_forwarding() {
        let ep = Endpoint::new("slow-peer");
        let server = IpcServer::start_at(ep.path(), || {}).expect("server binds");

        // Connect and send only 2 bytes of the 4-byte length prefix (never finishes).
        let mut slow = UnixStream::connect(ep.path()).unwrap();
        slow.write_all(&[0x00, 0x01]).unwrap();

        // A healthy client must still succeed immediately while the slow peer is parked.
        assert!(try_forward_to(ep.path(), "/tmp/healthy"));
        let drained = server.drain();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].path.to_path_buf(), PathBuf::from("/tmp/healthy"));

        drop(slow);
        drop(server);
    }

    #[test]
    fn oversized_frame_is_rejected() {
        let ep = Endpoint::new("oversize");
        let server = IpcServer::start_at(ep.path(), || {}).expect("server binds");

        let mut stream = UnixStream::connect(ep.path()).unwrap();
        stream.set_read_timeout(Some(IPC_IO_TIMEOUT)).unwrap();
        let bad_len = (MAX_IPC_FRAME_BYTES as u32) + 1;
        stream.write_all(&bad_len.to_be_bytes()).unwrap();
        let resp: IpcResponse = read_frame(&mut stream).unwrap();
        assert!(matches!(resp, IpcResponse::Error { .. }));
        assert!(server.drain().is_empty());
    }

    #[test]
    fn stale_file_is_cleaned_up_and_missing_socket_is_noop() {
        let ep = Endpoint::new("stale");
        assert!(!try_forward_to(ep.path(), "/tmp/nowhere"));

        fs::write(ep.path(), b"stale").unwrap();
        assert!(!try_forward_to(ep.path(), "/tmp/nowhere"));
        assert!(!ep.path().exists());
    }

    #[test]
    fn default_socket_path_never_uses_tmp() {
        let p = socket_path();
        assert!(
            p.ends_with(SOCKET_SUBPATH),
            "expected suffix {SOCKET_SUBPATH}, got {}",
            p.display()
        );
        if std::env::var("XDG_RUNTIME_DIR")
            .ok()
            .is_none_or(|d| !d.starts_with("/tmp"))
        {
            assert!(!p.starts_with("/tmp"));
        }
    }
}
