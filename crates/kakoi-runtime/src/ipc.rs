//! Bounded same-binary messages and owned descriptor transfer.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;

use serde::{de::DeserializeOwned, Serialize};

const MAX_MESSAGE: usize = 16 * 1024 * 1024;
const MAX_FDS: usize = 16;

fn invalid(detail: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, detail)
}

pub(crate) fn send<T: Serialize>(
    socket: &UnixStream,
    message: &T,
    fds: &[OwnedFd],
) -> io::Result<()> {
    send_watched(socket, message, fds, &[])
}

fn ready(socket: &UnixStream, events: i16, watches: &[RawFd]) -> io::Result<()> {
    let mut descriptors = vec![libc::pollfd {
        fd: socket.as_raw_fd(),
        events,
        revents: 0,
    }];
    descriptors.extend(watches.iter().map(|fd| libc::pollfd {
        fd: *fd,
        events: libc::POLLIN,
        revents: 0,
    }));
    if !watches.is_empty() {
        if let Some(fd) = kakoi_linux::supervisor_signals::descriptor() {
            descriptors.push(libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            });
        }
    }
    loop {
        if !watches.is_empty() && kakoi_linux::supervisor_signals::pending() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "supervisor received a terminal signal",
            ));
        }
        let count = unsafe { libc::poll(descriptors.as_mut_ptr(), descriptors.len() as _, -1) };
        if count < 0 {
            let cause = io::Error::last_os_error();
            if cause.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(cause);
        }
        if descriptors[1..].iter().any(|fd| fd.revents != 0) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "owner or control lost during transfer",
            ));
        }
        if descriptors[0].revents != 0 {
            return Ok(());
        }
    }
}

fn write_all(socket: &UnixStream, mut bytes: &[u8], watches: &[RawFd]) -> io::Result<()> {
    while !bytes.is_empty() {
        ready(socket, libc::POLLOUT, watches)?;
        let count = unsafe {
            libc::send(
                socket.as_raw_fd(),
                bytes.as_ptr().cast(),
                bytes.len(),
                libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL,
            )
        };
        if count < 0 {
            let cause = io::Error::last_os_error();
            if matches!(
                cause.kind(),
                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
            ) {
                continue;
            }
            return Err(cause);
        }
        if count == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }
        bytes = &bytes[count as usize..];
    }
    Ok(())
}

fn read_exact(socket: &UnixStream, mut bytes: &mut [u8], watches: &[RawFd]) -> io::Result<()> {
    while !bytes.is_empty() {
        ready(socket, libc::POLLIN, watches)?;
        let count = unsafe {
            libc::recv(
                socket.as_raw_fd(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
                libc::MSG_DONTWAIT,
            )
        };
        if count < 0 {
            let cause = io::Error::last_os_error();
            if matches!(
                cause.kind(),
                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
            ) {
                continue;
            }
            return Err(cause);
        }
        if count == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        bytes = &mut bytes[count as usize..];
    }
    Ok(())
}

pub(crate) fn send_watched<T: Serialize>(
    socket: &UnixStream,
    message: &T,
    fds: &[OwnedFd],
    watches: &[RawFd],
) -> io::Result<()> {
    let body = serde_json::to_vec(message)?;
    if body.len() > MAX_MESSAGE || fds.len() > MAX_FDS {
        return Err(invalid("internal message exceeds its declared limit"));
    }
    let mut header = [0u8; 8];
    header[..4].copy_from_slice(&(body.len() as u32).to_be_bytes());
    header[4..].copy_from_slice(&(fds.len() as u32).to_be_bytes());
    // usize alignment suffices for cmsghdr; only initialized header and fd bytes are sent.
    let mut ancillary = [0usize; 32];
    let mut iov = libc::iovec {
        iov_base: header.as_mut_ptr().cast(),
        iov_len: header.len(),
    };
    // SAFETY: all pointers refer to the buffers above, alive for the syscall.
    let sent = unsafe {
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        if !fds.is_empty() {
            msg.msg_control = ancillary.as_mut_ptr().cast();
            msg.msg_controllen =
                libc::CMSG_SPACE((fds.len() * std::mem::size_of::<libc::c_int>()) as u32) as _;
            let cmsg = libc::CMSG_FIRSTHDR(&msg);
            (*cmsg).cmsg_level = libc::SOL_SOCKET;
            (*cmsg).cmsg_type = libc::SCM_RIGHTS;
            (*cmsg).cmsg_len =
                libc::CMSG_LEN((fds.len() * std::mem::size_of::<libc::c_int>()) as u32) as _;
            let data = libc::CMSG_DATA(cmsg).cast::<libc::c_int>();
            for (index, fd) in fds.iter().enumerate() {
                data.add(index).write(fd.as_raw_fd());
            }
        }
        loop {
            ready(socket, libc::POLLOUT, watches)?;
            let sent = libc::sendmsg(
                socket.as_raw_fd(),
                &msg,
                libc::MSG_NOSIGNAL | libc::MSG_DONTWAIT,
            );
            if sent >= 0
                || !matches!(
                    io::Error::last_os_error().kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                )
            {
                break sent;
            }
        }
    };
    if sent < 0 {
        return Err(io::Error::last_os_error());
    }
    if sent == 0 {
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            "internal header was not sent",
        ));
    }
    write_all(socket, &header[sent as usize..], watches)?;
    write_all(socket, &body, watches)
}

pub(crate) fn recv<T: DeserializeOwned>(socket: &UnixStream) -> io::Result<(T, Vec<OwnedFd>)> {
    recv_watched(socket, &[])
}

pub(crate) fn recv_watched<T: DeserializeOwned>(
    socket: &UnixStream,
    watches: &[RawFd],
) -> io::Result<(T, Vec<OwnedFd>)> {
    let mut header = [0u8; 8];
    let mut ancillary = [0usize; 32];
    let mut iov = libc::iovec {
        iov_base: header.as_mut_ptr().cast(),
        iov_len: header.len(),
    };
    // SAFETY: recvmsg writes only within the supplied buffers. MSG_CMSG_CLOEXEC
    // closes the cross-thread exec race before received descriptors become visible.
    let (count, fds, truncated) = unsafe {
        let mut msg: libc::msghdr = std::mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = ancillary.as_mut_ptr().cast();
        msg.msg_controllen = std::mem::size_of_val(&ancillary) as _;
        let count = loop {
            ready(socket, libc::POLLIN, watches)?;
            let count = libc::recvmsg(
                socket.as_raw_fd(),
                &mut msg,
                libc::MSG_CMSG_CLOEXEC | libc::MSG_DONTWAIT,
            );
            if count >= 0
                || !matches!(
                    io::Error::last_os_error().kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                )
            {
                break count;
            }
        };
        if count < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut fds = Vec::new();
        let mut cmsg = libc::CMSG_FIRSTHDR(&msg);
        while !cmsg.is_null() {
            if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                let size = ((*cmsg).cmsg_len as usize).saturating_sub(libc::CMSG_LEN(0) as usize);
                let data = libc::CMSG_DATA(cmsg).cast::<libc::c_int>();
                for index in 0..size / std::mem::size_of::<libc::c_int>() {
                    fds.push(OwnedFd::from_raw_fd(data.add(index).read()));
                }
            }
            cmsg = libc::CMSG_NXTHDR(&msg, cmsg);
        }
        (count as usize, fds, msg.msg_flags & libc::MSG_CTRUNC != 0)
    };
    if count == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "internal peer closed",
        ));
    }
    if truncated {
        return Err(invalid("truncated internal descriptor transfer"));
    }
    read_exact(socket, &mut header[count..], watches)?;
    let size = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
    let declared = u32::from_be_bytes(header[4..].try_into().unwrap()) as usize;
    if size > MAX_MESSAGE || declared > MAX_FDS || declared != fds.len() {
        return Err(invalid(
            "internal message length or descriptor count mismatch",
        ));
    }
    let mut body = vec![0; size];
    read_exact(socket, &mut body, watches)?;
    Ok((serde_json::from_slice(&body)?, fds))
}
