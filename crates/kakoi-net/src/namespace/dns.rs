use super::{child_pidfd, duplicate_above_stdio, kill_and_reap, NetworkNamespace};
use std::{
    io,
    net::{TcpListener, UdpSocket},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::net::UnixDatagram,
    },
    sync::Arc,
    time::Duration,
};

/// Private resolver endpoint. Both A and AAAA questions can use this IPv4
/// loopback endpoint; upstream connections remain in the host controller.
pub struct DnsSockets {
    pub udp: UdpSocket,
    pub tcp: TcpListener,
    _namespace: Arc<NetworkNamespace>,
}

struct Helper {
    pid: libc::pid_t,
    pidfd: OwnedFd,
}

impl Drop for Helper {
    fn drop(&mut self) {
        kill_and_reap(&self.pidfd, self.pid);
    }
}

impl DnsSockets {
    pub fn bind(namespace: Arc<NetworkNamespace>) -> io::Result<Self> {
        let (parent, endpoint) = UnixDatagram::pair()?;
        parent.set_read_timeout(Some(Duration::from_secs(2)))?;
        let endpoint = duplicate_above_stdio(endpoint.as_raw_fd())?;
        let control = endpoint.as_raw_fd();
        let user = namespace.user.as_raw_fd();
        let net = namespace.net.as_raw_fd();
        // The short-lived child uses only raw syscalls and stack memory. Keeping
        // the host thread outside the child user namespace also preserves its
        // access to host DNS and avoids changing other threads' credentials.
        let pid = unsafe { libc::fork() };
        if pid < 0 {
            return Err(io::Error::last_os_error());
        }
        if pid == 0 {
            unsafe { bind_child(control, user, net) }
        }
        drop(endpoint);
        let pidfd = child_pidfd(pid)?;
        let _helper = Helper { pid, pidfd };
        let [udp, tcp] = receive(&parent)?;
        Ok(Self {
            udp: UdpSocket::from(udp),
            tcp: TcpListener::from(tcp),
            _namespace: namespace,
        })
    }
}

fn receive(socket: &UnixDatagram) -> io::Result<[OwnedFd; 2]> {
    let mut status = 0_i32;
    let mut control = [0_usize; 8];
    let mut vector = libc::iovec {
        iov_base: (&mut status as *mut i32).cast(),
        iov_len: 4,
    };
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut vector;
    message.msg_iovlen = 1;
    message.msg_control = control.as_mut_ptr().cast();
    message.msg_controllen = std::mem::size_of_val(&control) as _;
    let count = unsafe { libc::recvmsg(socket.as_raw_fd(), &mut message, libc::MSG_CMSG_CLOEXEC) };
    if count < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut descriptors = Vec::new();
    // Only the forked helper possesses the sending endpoint. Adopt all received
    // rights before validating the envelope so any error closes them via RAII.
    unsafe {
        let mut header = libc::CMSG_FIRSTHDR(&message);
        while !header.is_null() {
            if (*header).cmsg_level == libc::SOL_SOCKET && (*header).cmsg_type == libc::SCM_RIGHTS {
                let base = libc::CMSG_LEN(0) as usize;
                // glibc uses usize, while musl exposes socklen_t here.
                #[allow(clippy::unnecessary_cast)]
                let length = ((*header).cmsg_len as usize).saturating_sub(base);
                for index in 0..length / std::mem::size_of::<RawFd>() {
                    descriptors.push(OwnedFd::from_raw_fd(std::ptr::read_unaligned(
                        libc::CMSG_DATA(header).cast::<RawFd>().add(index),
                    )));
                }
            }
            header = libc::CMSG_NXTHDR(&message, header);
        }
    }
    if count != 4 || message.msg_flags & (libc::MSG_CTRUNC | libc::MSG_TRUNC) != 0 {
        return Err(io::Error::other("incomplete DNS socket transfer"));
    }
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status));
    }
    descriptors
        .try_into()
        .map_err(|_: Vec<OwnedFd>| io::Error::other("expected two DNS sockets"))
}

unsafe fn bind_child(control: RawFd, user: RawFd, net: RawFd) -> ! {
    let mut error = 0;
    if libc::setns(user, libc::CLONE_NEWUSER) != 0 || libc::setns(net, libc::CLONE_NEWNET) != 0 {
        error = *libc::__errno_location();
    }
    if libc::dup3(control, 3, libc::O_CLOEXEC) < 0
        || libc::syscall(libc::SYS_close_range, 4_u32, u32::MAX, 0) < 0
    {
        libc::_exit(125);
    }
    let address = libc::sockaddr_in {
        sin_family: libc::AF_INET as _,
        sin_port: 53_u16.to_be(),
        sin_addr: libc::in_addr {
            s_addr: u32::from_ne_bytes(kakoi_core::network::DNS_RESOLVER_ADDRESS.octets()),
        },
        sin_zero: [0; 8],
    };
    let mut sockets = [-1; 2];
    if error == 0 {
        for (index, kind) in [libc::SOCK_DGRAM, libc::SOCK_STREAM].iter().enumerate() {
            let fd = libc::socket(libc::AF_INET, *kind | libc::SOCK_CLOEXEC, 0);
            if fd < 0 {
                error = *libc::__errno_location();
                break;
            }
            sockets[index] = fd;
            if index == 1 {
                let enabled: libc::c_int = 1;
                if libc::setsockopt(
                    fd,
                    libc::SOL_SOCKET,
                    libc::SO_REUSEADDR,
                    (&enabled as *const libc::c_int).cast(),
                    std::mem::size_of_val(&enabled) as _,
                ) != 0
                {
                    error = *libc::__errno_location();
                    break;
                }
            }
            if libc::bind(
                fd,
                (&address as *const libc::sockaddr_in).cast(),
                std::mem::size_of_val(&address) as _,
            ) != 0
                || (index == 1 && libc::listen(fd, 128) != 0)
            {
                error = *libc::__errno_location();
                break;
            }
        }
    }
    let mut vector = libc::iovec {
        iov_base: (&mut error as *mut i32).cast(),
        iov_len: 4,
    };
    let mut ancillary = [0_usize; 8];
    let mut message: libc::msghdr = std::mem::zeroed();
    message.msg_iov = &mut vector;
    message.msg_iovlen = 1;
    if error == 0 {
        message.msg_control = ancillary.as_mut_ptr().cast();
        message.msg_controllen = libc::CMSG_SPACE(std::mem::size_of_val(&sockets) as _) as _;
        let header = libc::CMSG_FIRSTHDR(&message);
        (*header).cmsg_level = libc::SOL_SOCKET;
        (*header).cmsg_type = libc::SCM_RIGHTS;
        (*header).cmsg_len = libc::CMSG_LEN(std::mem::size_of_val(&sockets) as _) as _;
        std::ptr::copy_nonoverlapping(sockets.as_ptr(), libc::CMSG_DATA(header).cast::<RawFd>(), 2);
    }
    let sent = libc::sendmsg(3, &message, libc::MSG_NOSIGNAL);
    libc::_exit(if sent == 4 && error == 0 { 0 } else { 125 });
}
