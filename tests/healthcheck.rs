use home_tracker::healthcheck::probe;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::thread;
use std::time::Duration;

#[test]
fn probe_succeeds_against_a_live_listener() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        if let Ok((mut sock, _)) = listener.accept() {
            let mut buf = [0u8; 64];
            let _ = sock.read(&mut buf);
            let _ = sock.write_all(b"HTTP/1.0 404 Not Found\r\n\r\n");
        }
    });
    // Any HTTP answer, even a 404, proves the process is up.
    assert!(probe(addr, Duration::from_secs(3)).is_ok());
    handle.join().unwrap();
}

#[test]
fn probe_fails_against_a_closed_port() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    drop(listener);
    assert!(probe(addr, Duration::from_millis(500)).is_err());
}

#[test]
fn probe_fails_when_the_server_closes_without_answering() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        if let Ok((sock, _)) = listener.accept() {
            drop(sock);
        }
    });
    assert!(probe(addr, Duration::from_secs(3)).is_err());
    handle.join().unwrap();
}
