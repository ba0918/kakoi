use super::remaining;
use crate::dns_workers::Cancellation;
use std::{
    io,
    net::{SocketAddr, TcpStream},
    os::fd::{AsRawFd, RawFd},
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub(super) struct ExchangeDeadline<'a> {
    at: Instant,
    cancellation: Option<&'a Cancellation>,
}

impl<'a> ExchangeDeadline<'a> {
    pub(super) fn new(at: Instant, cancellation: Option<&'a Cancellation>) -> Self {
        Self { at, cancellation }
    }

    pub(super) fn remaining(self) -> io::Result<Duration> {
        if self.cancellation.is_some_and(Cancellation::is_cancelled) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "DNS resolution cancelled",
            ));
        }
        remaining(self.at)
    }

    pub(super) fn wait(self, fd: RawFd, events: i16) -> io::Result<()> {
        loop {
            let mut timeout = self.remaining()?;
            if self.cancellation.is_some() {
                timeout = timeout.min(Duration::from_millis(25));
            }
            let millis = timeout.as_millis().saturating_add(1).min(i32::MAX as u128) as i32;
            let mut descriptor = libc::pollfd {
                fd,
                events,
                revents: 0,
            };
            let result = unsafe { libc::poll(&mut descriptor, 1, millis) };
            self.remaining()?;
            if result > 0 {
                if descriptor.revents & libc::POLLNVAL != 0 {
                    return Err(io::Error::other("invalid DNS socket"));
                }
                return Ok(());
            }
            if result < 0 {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    return Err(error);
                }
            }
        }
    }

    pub(super) fn connect(self, peer: SocketAddr) -> io::Result<TcpStream> {
        self.remaining()?;
        // std's connect_timeout cannot observe cancellation during connect.
        // socket2 owns address conversion and safe socket creation; poll waits
        // on this single attempt rather than opening a new connection per tick.
        let socket = socket2::Socket::new(
            socket2::Domain::for_address(peer),
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )?;
        socket.set_nonblocking(true)?;
        if let Err(error) = socket.connect(&peer.into()) {
            if !matches!(
                error.raw_os_error(),
                Some(libc::EINPROGRESS | libc::EALREADY | libc::EINTR)
            ) {
                return Err(error);
            }
            self.wait(socket.as_raw_fd(), libc::POLLOUT)?;
            if let Some(error) = socket.take_error()? {
                return Err(error);
            }
        }
        self.remaining()?;
        socket.peer_addr()?;
        Ok(socket.into())
    }
}
