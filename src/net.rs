//! Listener setup for the HTTP server.
//!
//! The server listens on IPv4 and IPv6 at the same time. It does so with a
//! *single* dual-stack socket rather than one socket per family: an IPv6 socket
//! with `IPV6_V6ONLY` cleared accepts native IPv6 connections and IPv4 ones
//! alike, the latter reported as IPv4-mapped addresses (`::ffff:192.0.2.1`).
//!
//! `IPV6_V6ONLY` is set explicitly rather than left to the platform. Linux
//! derives its default from the `net.ipv6.bindv6only` sysctl, so a bare `[::]`
//! bind is dual-stack on a stock host but silently IPv6-only on a hardened one.
//! Setting it ourselves makes the behaviour a property of this program.

use std::net::{IpAddr, SocketAddr};

use anyhow::{Context, Result};
use socket2::{Domain, Socket, Type};

/// Backlog for `listen`, matching the value tokio's own `TcpListener::bind` uses.
const LISTEN_BACKLOG: i32 = 1024;

/// Builds the address to bind from the `LISTEN_ADDRESS` and `PORT` settings.
///
/// IPv6 literals are accepted bare (`::`) or bracketed (`[::]`), since both
/// spellings are natural to write in an env var.
///
/// Note this parses into a `SocketAddr` rather than formatting `"{addr}:{port}"`
/// and letting the bind parse it: that produced `":::7007"` for `::`, which is
/// not a valid socket address, so an IPv6 `LISTEN_ADDRESS` could never work.
pub fn listen_addr(listen_address: &str, port: u16) -> Result<SocketAddr> {
    let trimmed = listen_address.trim();
    let unbracketed = trimmed
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(trimmed);

    let ip: IpAddr = unbracketed
        .parse()
        .with_context(|| format!("LISTEN_ADDRESS is not a valid IP address: {listen_address:?}"))?;

    Ok(SocketAddr::new(ip, port))
}

/// Binds a listening socket, dual-stack when `addr` is IPv6.
///
/// Returns a blocking [`std::net::TcpListener`]; the caller converts it with
/// `tokio::net::TcpListener::from_std`.
pub fn bind(addr: SocketAddr) -> Result<std::net::TcpListener> {
    let domain = Domain::for_address(addr);
    let socket = Socket::new(domain, Type::STREAM, None)
        .with_context(|| format!("Failed to create a socket for {addr}"))?;

    if addr.is_ipv6() {
        socket
            .set_only_v6(false)
            .with_context(|| format!("Failed to enable dual-stack listening on {addr}"))?;
    }

    // tokio's own bind does this on non-Windows platforms; without it a restart
    // fails while the previous socket sits in TIME_WAIT.
    #[cfg(not(windows))]
    socket
        .set_reuse_address(true)
        .with_context(|| format!("Failed to set SO_REUSEADDR on {addr}"))?;

    socket
        .bind(&addr.into())
        .with_context(|| format!("Failed to bind to {addr}"))?;
    socket
        .listen(LISTEN_BACKLOG)
        .with_context(|| format!("Failed to listen on {addr}"))?;
    socket
        .set_nonblocking(true)
        .with_context(|| format!("Failed to set {addr} non-blocking"))?;

    Ok(socket.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn parses_the_dual_stack_default() {
        let addr = listen_addr("::", 7008).unwrap();
        assert_eq!(
            addr,
            SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 7008)
        );
        assert!(addr.is_ipv6());
    }

    #[test]
    fn accepts_a_bracketed_ipv6_address() {
        assert_eq!(
            listen_addr("[::]", 7008).unwrap(),
            listen_addr("::", 7008).unwrap()
        );
        assert_eq!(
            listen_addr("[::1]", 80).unwrap(),
            listen_addr("::1", 80).unwrap()
        );
    }

    #[test]
    fn still_honours_an_explicit_ipv4_address() {
        let addr = listen_addr("0.0.0.0", 7008).unwrap();
        assert_eq!(
            addr,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 7008)
        );
        // An operator who pins to IPv4 gets IPv4, not a surprise dual-stack socket.
        assert!(addr.is_ipv4());

        assert_eq!(
            listen_addr("127.0.0.1", 1234).unwrap(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1234)
        );
    }

    #[test]
    fn tolerates_surrounding_whitespace() {
        assert_eq!(
            listen_addr("  ::  ", 7008).unwrap(),
            listen_addr("::", 7008).unwrap()
        );
    }

    #[test]
    fn rejects_a_value_that_is_not_an_ip_address() {
        // Notably a host:port pair, which is the shape the old string-concatenating
        // code would happily have produced nonsense from.
        for bad in ["localhost", "0.0.0.0:7008", "1.2.3.4.5", ""] {
            let err = listen_addr(bad, 7008).unwrap_err();
            assert!(
                err.to_string().contains("not a valid IP address"),
                "expected a parse error for {bad:?}, got: {err}"
            );
        }
    }

    /// `LISTEN_ADDRESS` carries an address only - never a port. Worth pinning,
    /// because `::7008` looks like it means "port 7008" and is in fact a
    /// perfectly valid IPv6 address whose last group is 0x7008, so a typo like
    /// that binds somewhere unintended instead of failing loudly.
    #[test]
    fn treats_a_bare_ipv6_value_as_an_address_not_a_port() {
        let addr = listen_addr("::7008", 1234).unwrap();
        assert_eq!(addr.port(), 1234);
        assert_eq!(addr.ip(), IpAddr::V6("::7008".parse::<Ipv6Addr>().unwrap()));
    }

    /// The whole point of the change: one socket, both families.
    #[test]
    fn an_ipv6_listener_accepts_both_ipv4_and_ipv6_clients() {
        use std::io::{Read, Write};
        use std::net::TcpStream;

        // Port 0 lets the OS pick a free one, so the test cannot collide with a
        // running server or with a parallel test.
        let listener = bind(listen_addr("::", 0).unwrap()).unwrap();
        let port = listener.local_addr().unwrap().port();

        for target in [
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port),
            SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port),
        ] {
            let mut client = TcpStream::connect(target)
                .unwrap_or_else(|e| panic!("could not connect to {target}: {e}"));
            client.write_all(b"hi").unwrap();

            let (mut server_side, _peer) = listener.accept().unwrap();
            let mut buf = [0u8; 2];
            server_side.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, b"hi");
        }
    }

    #[test]
    fn an_ipv4_listener_does_not_accept_ipv6_clients() {
        use std::net::TcpStream;

        let listener = bind(listen_addr("127.0.0.1", 0).unwrap()).unwrap();
        let port = listener.local_addr().unwrap().port();

        assert!(
            TcpStream::connect(SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), port)).is_err()
        );
    }
}
