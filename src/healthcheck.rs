//! Liveness probe for the Docker `HEALTHCHECK`. The `scratch` image has no shell
//! or curl, so the check execs this binary with the `healthcheck` subcommand.

use std::io::{self, Read, Write};
use std::net::{Ipv6Addr, SocketAddr, TcpStream};
use std::process;
use std::time::Duration;

fn loopback_v4(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

fn loopback_v6(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv6Addr::LOCALHOST, port))
}

/// Only "nothing listens on this family" errors are worth retrying on the other
/// loopback. A timeout has already spent the budget; retrying would get the
/// Docker check killed instead of reported.
fn worth_retrying(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::AddrNotAvailable
            | io::ErrorKind::NetworkUnreachable
            | io::ErrorKind::HostUnreachable
    )
}

/// Probe `127.0.0.1:port`, then `[::1]:port` if IPv4 is not listening at all.
pub fn probe_local(port: u16, timeout: Duration) -> io::Result<()> {
    match probe(loopback_v4(port), timeout) {
        Err(e) if worth_retrying(&e) => probe(loopback_v6(port), timeout),
        other => other,
    }
}

/// Connect, send a minimal request, and require at least one byte back.
pub fn probe(addr: SocketAddr, timeout: Duration) -> io::Result<()> {
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n")?;
    let mut buf = [0u8; 16];
    let n = stream.read(&mut buf)?;
    if n == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "empty response from server",
        ));
    }
    Ok(())
}

/// Exit 0 when healthy, 1 otherwise.
pub fn run(port: u16) -> ! {
    match probe_local(port, Duration::from_secs(3)) {
        Ok(()) => process::exit(0),
        Err(e) => {
            eprintln!("healthcheck failed: {e}");
            process::exit(1);
        }
    }
}
