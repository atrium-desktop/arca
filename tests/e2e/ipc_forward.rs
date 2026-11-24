//! End-to-end single-instance forwarding.
//!
//! A second `arca <path>` invocation must connect to the running window's Unix
//! socket and exit without opening a window. The suite plays the window: it
//! binds the endpoint, spawns the real binary, accepts the handshake, and
//! asserts the absolute path the window would open.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use test_support::fs::TestDir;

#[test]
fn a_second_invocation_forwards_its_path_and_exits() {
    let dir = TestDir::new("e2e-ipc");
    let runtime = dir.subdir("runtime");
    let socket = runtime.join("arca-ipc.sock");
    let listener = UnixListener::bind(&socket).expect("bind the fake window endpoint");
    let scratch = dir.subdir("scratch");

    let (sender, receiver) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept the forwarded request");
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).expect("read the request line");
        let mut writer = reader.into_inner();
        writer.write_all(b"OK\n").expect("acknowledge the forward");
        sender.send(line).expect("send request line");
    });

    let mut child = Command::new(crate::common::binary("arca"))
        .arg("relative-target")
        .current_dir(&scratch)
        .env("XDG_RUNTIME_DIR", &runtime)
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

    let line = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("server received the request");
    server.join().expect("server thread");
    let path = line
        .trim()
        .strip_prefix("OPEN\t")
        .expect("request uses the OPEN verb");
    assert!(
        path.starts_with('/'),
        "the window receives an absolute path, got {path:?}"
    );
    assert!(
        path.ends_with("/relative-target"),
        "the client resolves relative arguments against its own cwd, got {path:?}"
    );
    assert!(
        path.starts_with(scratch.to_str().unwrap()),
        "the path is resolved inside the client cwd {scratch:?}, got {path:?}"
    );
}
