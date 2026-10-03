//! Bounded same-binary messages and owned descriptor transfer.

use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
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
            let sent = libc::sendmsg(socket.as_raw_fd(), &msg, libc::MSG_NOSIGNAL);
            if sent >= 0 || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
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
    let mut socket = socket;
    socket.write_all(&header[sent as usize..])?;
    socket.write_all(&body)
}

pub(crate) fn recv<T: DeserializeOwned>(socket: &UnixStream) -> io::Result<(T, Vec<OwnedFd>)> {
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
            let count = libc::recvmsg(socket.as_raw_fd(), &mut msg, libc::MSG_CMSG_CLOEXEC);
            if count >= 0 || io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
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
    let mut socket = socket;
    socket.read_exact(&mut header[count..])?;
    let size = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
    let declared = u32::from_be_bytes(header[4..].try_into().unwrap()) as usize;
    if size > MAX_MESSAGE || declared > MAX_FDS || declared != fds.len() {
        return Err(invalid(
            "internal message length or descriptor count mismatch",
        ));
    }
    let mut body = vec![0; size];
    socket.read_exact(&mut body)?;
    Ok((serde_json::from_slice(&body)?, fds))
}
