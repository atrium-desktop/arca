//! Integration coverage for the single-instance IPC socket.
//!
//! A command-line invocation forwards its path to a running window instead of
//! opening a second instance. The endpoint is a plain Unix socket with a
//! line-oriented request/response, so the suite can drive both ends in
//! process: the same code paths the binary and the window use.

use arca_ui::ipc::{IpcServer, try_forward_to};
use test_support::fs::TestDir;
use test_support::poll;

fn endpoint(dir: &TestDir, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    path
}

#[test]
fn forwarding_roundtrip_queues_each_path_in_order() {
    let dir = TestDir::new("ipc");
    let socket = endpoint(&dir, "arca.sock");
    let server = IpcServer::start_at(&socket).expect("bind the IPC endpoint");

    for path in ["/tmp/one", "/tmp/two", "/tmp/three"] {
        assert!(try_forward_to(&socket, path));
    }
    let drained: Vec<String> = server
        .drain()
        .into_iter()
        .map(|req| req.path.to_path_buf().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        drained,
        vec!["/tmp/one", "/tmp/two", "/tmp/three"],
        "the window drains every forwarded path in arrival order"
    );
    assert!(server.drain().is_empty(), "draining is destructive");
}

#[test]
fn stale_endpoint_is_rejected_and_removed() {
    let dir = TestDir::new("ipc-stale");
    let socket = endpoint(&dir, "arca.sock");
    std::fs::write(&socket, b"not a socket").expect("write stale file");

    assert!(
        !try_forward_to(&socket, "/tmp/nowhere"),
        "a regular file must not accept a forward"
    );
    assert!(!socket.exists(), "the dead endpoint is cleaned up");
}

#[test]
fn missing_endpoint_is_a_clean_no_op() {
    let dir = TestDir::new("ipc-missing");
    let socket = endpoint(&dir, "absent.sock");
    assert!(!try_forward_to(&socket, "/tmp/nowhere"));
}

#[test]
fn many_clients_are_queued_without_loss() {
    let dir = TestDir::new("ipc-stress");
    let socket = endpoint(&dir, "arca.sock");
    let server = IpcServer::start_at(&socket).expect("bind the IPC endpoint");

    let mut clients = Vec::new();
    for client in 0..8 {
        let socket = socket.clone();
        clients.push(std::thread::spawn(move || {
            for index in 0..10 {
                assert!(
                    try_forward_to(&socket, &format!("/tmp/client-{client}-{index}")),
                    "client {client} forward {index} was refused"
                );
            }
        }));
    }
    for client in clients {
        client.join().expect("client thread");
    }

    let mut received = 0usize;
    assert!(
        poll::wait_until(|| {
            received += server.drain().len();
            received >= 80
        }),
        "every forwarded path must be delivered, saw {received}"
    );
}

#[test]
fn dropping_the_server_unlinks_the_socket() {
    let dir = TestDir::new("ipc-drop");
    let socket = endpoint(&dir, "arca.sock");
    let server = IpcServer::start_at(&socket).expect("bind the IPC endpoint");
    assert!(socket.exists());
    drop(server);
    assert!(!socket.exists(), "drop unlinks the endpoint");
}
