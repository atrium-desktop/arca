//! Thin UI wake-callback adapter over [`arca_engine::ipc`] (`arca-ipc-v2`, ADR-0023).
//!
//! All protocol framing, `SO_PEERCRED` verification, `0600` socket permissions,
//! timeouts, and `XDG_ACTIVATION_TOKEN` propagation live in `arca-engine::ipc`.

use std::path::{Path, PathBuf};

pub use arca_engine::ipc::{
    ForwardedOpen, IPC_PROTOCOL_VERSION, socket_path, try_forward_bytes, try_forward_to,
    try_forward_to_existing,
};

/// Wake the Optics main loop on a best-effort basis when a peer forwards an
/// open request. Returns silently in headless tests where no run loop exists.
fn wake_main_thread() {
    let _ = optics_window::post_to_main_thread(|| {});
}

/// Background IPC server instance for the active Arca window.
pub struct IpcServer(arca_engine::ipc::IpcServer);

impl IpcServer {
    /// Start the Unix domain socket listener for this instance.
    pub fn start() -> Option<Self> {
        arca_engine::ipc::IpcServer::start(wake_main_thread).map(Self)
    }

    /// Start on an explicit socket path (tests).
    pub fn start_at(sock: &Path) -> Option<Self> {
        arca_engine::ipc::IpcServer::start_at(sock, wake_main_thread).map(Self)
    }

    /// Take the forwarded open requests received since the last drain.
    pub fn drain(&self) -> Vec<ForwardedOpen> {
        self.0.drain()
    }

    /// Convenience helper returning drained paths as lossy UTF-8 strings.
    pub fn drain_paths(&self) -> Vec<PathBuf> {
        self.0
            .drain()
            .into_iter()
            .map(|req| req.path.to_path_buf())
            .collect()
    }
}
