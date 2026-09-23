//! The addresses that stand for the host's loopback inside a filtered environment.
//! The outer pasta translates only these to the host's 127.0.0.1 and ::1; neither
//! stage translates the gateway, which therefore keeps meaning the real gateway.

use std::net::{Ipv4Addr, Ipv6Addr};

/// Answered for `host-v4.kakoi.internal`.
pub const HOST_LOOPBACK_V4: Ipv4Addr = Ipv4Addr::new(169, 254, 1, 2);
/// Answered for `host-v6.kakoi.internal`.
pub const HOST_LOOPBACK_V6: Ipv6Addr = Ipv6Addr::new(0xfd6b, 0x616b, 0x6f69, 0, 0, 0, 0, 2);
