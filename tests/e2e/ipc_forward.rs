//! End-to-end single-instance forwarding (ADR-0023 `arca-ipc-v2`).
//!
//! A second `arca <path>` invocation must connect to the running window's
//! private Unix socket (`$XDG_RUNTIME_DIR/arca/ipc.sock`) and exit without
//! opening a window. The suite plays the window: it starts an `IpcServer`,
//! spawns the real binary, and asserts the forwarded path and activation token
//! the window would receive.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use arca_engine::ipc::{IpcServer, SOCKET_SUBPATH};
use test_support::fs::TestDir;

#[test]
fn a_second_invocation_forwards_its_path_and_exits() {
    let dir = TestDir::new("e2e-ipc");
    let runtime = dir.subdir("runtime");
    let socket = runtime.join(SOCKET_SUBPATH);
    let server = IpcServer::start_at(&socket, || {}).expect("bind the fake window endpoint");
    let scratch = dir.subdir("scratch");

    let mut child = Command::new(crate::common::binary("arca"))
        .arg("relative-target")
        .current_dir(&scratch)
        .env("XDG_RUNTIME_DIR", &runtime)
        .env("XDG_ACTIVATION_TOKEN", "e2e-token")
        .env_remove("WAYLAND_DISPLAY")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn arca");

    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        match child.try_wait().expect("poll child") {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                panic!("a forwarded invocation must exit instead of opening a window");
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    let output = child.wait_with_output().expect("collect child output");
    assert!(
        status.success(),
        "forwarded invocation failed: stderr={:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    let forwarded = server.drain();
    assert_eq!(
        forwarded.len(),
        1,
        "server should receive exactly one forwarded open request"
    );
    let req = &forwarded[0];
    let path = req.path.to_path_buf();
    assert!(
        path.is_absolute(),
        "the window receives an absolute path, got {path:?}"
    );
    assert!(
        path.ends_with("relative-target"),
        "the client resolves relative arguments against its own cwd, got {path:?}"
    );
    assert!(
        path.starts_with(&scratch),
        "the path is resolved inside the client cwd {scratch:?}, got {path:?}"
    );
    assert_eq!(
        req.activation_token.as_deref(),
        Some("e2e-token"),
        "the client forwards XDG_ACTIVATION_TOKEN to the running window"
    );
}
