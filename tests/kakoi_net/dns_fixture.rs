//! What the DNS tests build by hand: messages, independent of the product's own
//! encoding, the permission that authorizes names, the upstream they are sent to, and
//! the namespace DNS is served in.

use kakoi_net::{filter, namespace::NetworkNamespace, nft};
use kakoi_runtime::cli::network::{Allow, Destination, DnsUpstream, Protocol};
use std::{
    net::Ipv4Addr,
    num::NonZeroU16,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

/// Permission for TCP 443 of the names `pattern` matches.
pub(crate) fn dns_rule(pattern: &str) -> Allow {
    Allow {
        destination: Destination::Dns(pattern.parse().unwrap()),
        protocol: Protocol::Tcp,
        ports: vec!["443".into()].try_into().unwrap(),
    }
}

/// `owner` as uncompressed labels, ending with the root label.
pub(crate) fn name(owner: &str) -> Vec<u8> {
    let mut wire = Vec::new();
    for label in owner.split('.') {
        wire.push(label.len() as u8);
        wire.extend(label.as_bytes());
    }
    wire.push(0);
    wire
}

/// A query with ID 0x1234 and recursion desired, for `owner` of type `kind` in class IN.
pub(crate) fn query(owner: &str, kind: u16) -> Vec<u8> {
    let mut wire = vec![0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    wire.extend(name(owner));
    wire.extend(kind.to_be_bytes());
    wire.extend([0, 1]);
    wire
}

/// The response to `question` with one A record of `address` for `ttl` seconds, its owner
/// a pointer to the question's name.
pub(crate) fn a_answer(question: &[u8], ttl: u32, address: [u8; 4]) -> Vec<u8> {
    let mut answer = question.to_vec();
    answer[2] |= 0x80;
    answer[7] = 1;
    answer.extend([0xc0, 0x0c, 0, 1, 0, 1]);
    answer.extend(ttl.to_be_bytes());
    answer.extend([0, 4]);
    answer.extend(address);
    answer
}

/// The plain upstream at 127.0.0.1:`port`.
pub(crate) fn local_upstream(port: u16) -> Vec<DnsUpstream> {
    vec![DnsUpstream::plain(
        Ipv4Addr::LOCALHOST.into(),
        NonZeroU16::new(port).unwrap(),
    )]
}

/// A network namespace of its own with loopback up.
pub(crate) fn loopback_namespace() -> Arc<NetworkNamespace> {
    let namespace = Arc::new(NetworkNamespace::create().unwrap());
    assert!(namespace
        .command("/usr/sbin/ip")
        .unwrap()
        .args(["link", "set", "lo", "up"])
        .status()
        .unwrap()
        .success());
    namespace
}

/// A [`loopback_namespace`] with the static filter of no allow rules applied.
pub(crate) fn filtered_namespace() -> Arc<NetworkNamespace> {
    let namespace = loopback_namespace();
    nft::apply(
        &namespace,
        Path::new("/usr/sbin/nft"),
        &filter::compile_static(&[], 120).unwrap(),
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    namespace
}
