//! The addresses that stand for the host's loopback inside a filtered environment.
//! The outer pasta translates only these to the host's 127.0.0.1 and ::1; neither
//! stage translates the gateway, which therefore keeps meaning the real gateway.
//! Also the addresses the host itself holds, which only an IP permission opens.

use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

/// Answered for `host-v4.kakoi.internal`.
pub const HOST_LOOPBACK_V4: Ipv4Addr = Ipv4Addr::new(169, 254, 1, 2);
/// Answered for `host-v6.kakoi.internal`.
pub const HOST_LOOPBACK_V6: Ipv6Addr = Ipv6Addr::new(0xfd6b, 0x616b, 0x6f69, 0, 0, 0, 0, 2);

/// Every address on the calling thread's network namespace's interfaces, read
/// once. Addresses the host gains later are not seen.
pub fn addresses() -> io::Result<Vec<IpAddr>> {
    let mut list = std::ptr::null_mut();
    // SAFETY: getifaddrs fills `list` with a list freed below, once.
    if unsafe { libc::getifaddrs(&mut list) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut addresses = Vec::new();
    let mut entry = list;
    while !entry.is_null() {
        // SAFETY: `entry` is a node of the list getifaddrs returned, not yet freed;
        // `ifa_addr` is null or points to a socket address of the family it names.
        unsafe {
            let address = (*entry).ifa_addr;
            if !address.is_null() {
                match i32::from((*address).sa_family) {
                    libc::AF_INET => {
                        let v4 = &*(address as *const libc::sockaddr_in);
                        addresses
                            .push(IpAddr::V4(Ipv4Addr::from(u32::from_be(v4.sin_addr.s_addr))));
                    }
                    libc::AF_INET6 => {
                        let v6 = &*(address as *const libc::sockaddr_in6);
                        addresses.push(IpAddr::V6(Ipv6Addr::from(v6.sin6_addr.s6_addr)));
                    }
                    _ => {}
                }
            }
            entry = (*entry).ifa_next;
        }
    }
    // SAFETY: `list` came from getifaddrs and is freed only here.
    unsafe { libc::freeifaddrs(list) };
    Ok(addresses)
}
