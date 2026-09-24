//! Upstream exchanges for one question: trying the candidates in order, reserving
//! each query from the shared budget, and falling back to TCP within a candidate's
//! wait. Policy authorization and settings generations belong to the resolution
//! coordinator.

use crate::{
    dns::{Question, ResponseDisposition, ValidatedResponse},
    resolution::{ResolutionBudget, UpstreamWait},
};
use std::{
    collections::BTreeSet,
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream, UdpSocket},
    os::fd::AsRawFd,
    time::{Duration, Instant},
};

mod control;
use crate::dns_workers::Cancellation;
use control::ExchangeDeadline;
mod tls;
pub use tls::TlsClient;

/// The instant the complete DNS wire message was read, before DNS validation.
/// TTL accounting uses this timestamp rather than a later adoption/cache time.
pub struct ReceivedResponse {
    pub response: ValidatedResponse,
    pub received_at: Instant,
}

impl ReceivedResponse {
    pub fn wire(&self) -> &[u8] {
        self.response.wire()
    }
    pub fn disposition(&self) -> ResponseDisposition {
        self.response.disposition()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PlainTransport {
    Udp,
    Tcp,
}

/// Use the configured transport for every candidate. `trust` is captured once by
/// the controller at startup; absence of TLS trust is an error, never a downgrade.
pub fn exchange_upstreams(
    wire: &[u8],
    upstreams: &[kakoi_core::network::DnsUpstream],
    trust: Option<&TlsClient>,
    budget: &mut ResolutionBudget,
) -> io::Result<ReceivedResponse> {
    exchange_upstreams_cancellable(wire, upstreams, trust, budget, None, UpstreamWait::Explicit)
}

pub(crate) fn exchange_upstreams_cancellable(
    wire: &[u8],
    upstreams: &[kakoi_core::network::DnsUpstream],
    trust: Option<&TlsClient>,
    budget: &mut ResolutionBudget,
    cancellation: Option<&Cancellation>,
    wait: UpstreamWait,
) -> io::Result<ReceivedResponse> {
    kakoi_core::network::validate_upstreams(upstreams)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let Some(first) = upstreams.first() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "no explicit DNS upstreams",
        ));
    };
    if first.tls_name().is_none() {
        let peers: Vec<_> = upstreams
            .iter()
            .map(|upstream| SocketAddr::new(upstream.address, upstream.port().get()))
            .collect();
        return plain_candidates(wire, &peers, budget, cancellation, wait);
    }
    let trust = trust.ok_or_else(|| io::Error::other("TLS DNS requires startup CA snapshot"))?;
    Question::parse(wire).map_err(invalid_dns)?;
    let mut attempted = BTreeSet::new();
    let mut last_error = io::Error::other("no usable TLS DNS upstream");
    for upstream in upstreams {
        let peer = SocketAddr::new(upstream.address, upstream.port().get());
        let name = upstream
            .tls_name()
            .expect("homogeneous validated TLS upstreams");
        if !attempted.insert((peer, name)) {
            continue;
        }
        let deadline = budget
            .reserve_query(Instant::now(), wait)
            .map_err(|limit| io::Error::other(format!("DNS resolution limit: {limit:?}")))?;
        match trust.exchange_controlled(
            wire,
            peer,
            name,
            ExchangeDeadline::new(deadline, cancellation),
        ) {
            Ok(answer) if answer.disposition() == ResponseDisposition::Answer => return Ok(answer),
            Ok(_) => {
                last_error = io::Error::other("TLS DNS upstream failed or returned incomplete data")
            }
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

/// One question against fixed plain candidates, each given one candidate's wait.
/// CNAME traversal must keep the same `budget`. The host's DNS reaches the same
/// candidate loop through `exchange_upstreams_cancellable`, with its own wait.
pub fn exchange_plain_candidates(
    wire: &[u8],
    peers: &[SocketAddr],
    budget: &mut ResolutionBudget,
) -> io::Result<ReceivedResponse> {
    plain_candidates(wire, peers, budget, None, UpstreamWait::Explicit)
}

fn plain_candidates(
    wire: &[u8],
    peers: &[SocketAddr],
    budget: &mut ResolutionBudget,
    cancellation: Option<&Cancellation>,
    wait: UpstreamWait,
) -> io::Result<ReceivedResponse> {
    Question::parse(wire).map_err(invalid_dns)?;
    let mut attempted = BTreeSet::new();
    let mut last_error = io::Error::other("no usable DNS upstream");
    for peer in peers {
        if !attempted.insert(*peer) {
            continue;
        }
        let candidate_deadline = budget
            .reserve_query(Instant::now(), wait)
            .map_err(|limit| io::Error::other(format!("DNS resolution limit: {limit:?}")))?;
        let mut result = plain(
            wire,
            *peer,
            PlainTransport::Udp,
            ExchangeDeadline::new(candidate_deadline, cancellation),
        );
        if result
            .as_ref()
            .is_ok_and(|answer| answer.disposition() == ResponseDisposition::RetryTcp)
        {
            // Changing transport neither renews this candidate's wait nor resets
            // the resolution's query count.
            if remaining(candidate_deadline).is_err() {
                last_error = io::Error::new(io::ErrorKind::TimedOut, "DNS candidate deadline");
                continue;
            }
            let deadline = budget
                .reserve_query(Instant::now(), wait)
                .map_err(|limit| io::Error::other(format!("DNS resolution limit: {limit:?}")))?
                .min(candidate_deadline);
            result = plain(
                wire,
                *peer,
                PlainTransport::Tcp,
                ExchangeDeadline::new(deadline, cancellation),
            );
        }
        match result {
            Ok(answer) if answer.disposition() == ResponseDisposition::Answer => return Ok(answer),
            Ok(_) => {
                last_error = io::Error::other("DNS upstream failed or returned incomplete data")
            }
            Err(error) => last_error = error,
        }
    }
    Err(last_error)
}

/// Call only for an authorized question, immediately after reserving one upstream
/// query. The socket lives in the calling thread's network namespace. No fallback
/// or resend is hidden here, so each extra query can consume the shared budget.
pub fn exchange_plain(
    wire: &[u8],
    peer: SocketAddr,
    transport: PlainTransport,
    deadline: Instant,
) -> io::Result<ReceivedResponse> {
    plain(wire, peer, transport, ExchangeDeadline::new(deadline, None))
}

fn plain(
    wire: &[u8],
    peer: SocketAddr,
    transport: PlainTransport,
    deadline: ExchangeDeadline<'_>,
) -> io::Result<ReceivedResponse> {
    let original = Question::parse(wire).map_err(invalid_dns)?;
    deadline.remaining()?;
    let mut request = wire.to_vec();
    random_id(&mut request[..2], deadline)?;
    let question = Question::parse(&request).map_err(invalid_dns)?;
    let (mut answer, received_at) = match transport {
        PlainTransport::Udp => udp(&request, &question, peer, deadline)?,
        PlainTransport::Tcp => tcp(&request, &question, peer, deadline)?,
    };
    deadline.remaining()?;
    answer[..2].copy_from_slice(&wire[..2]);
    Ok(ReceivedResponse {
        response: original.validate_response(&answer).map_err(invalid_dns)?,
        received_at,
    })
}

fn invalid_dns(error: crate::dns::DnsError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("DNS: {error:?}"))
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|value| !value.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "DNS exchange deadline"))
}

fn random_id(mut bytes: &mut [u8], deadline: ExchangeDeadline<'_>) -> io::Result<()> {
    while !bytes.is_empty() {
        deadline.remaining()?;
        // Nonblocking entropy acquisition cannot outlive the resolution deadline.
        let count =
            unsafe { libc::getrandom(bytes.as_mut_ptr().cast(), bytes.len(), libc::GRND_NONBLOCK) };
        if count > 0 {
            bytes = &mut bytes[count as usize..];
        } else if count == 0 {
            return Err(io::Error::other("getrandom returned no bytes"));
        } else {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
    Ok(())
}

fn udp(
    request: &[u8],
    question: &Question,
    peer: SocketAddr,
    deadline: ExchangeDeadline<'_>,
) -> io::Result<(Vec<u8>, Instant)> {
    let socket = UdpSocket::bind(if peer.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })?;
    socket.set_nonblocking(true)?;
    // A connected UDP socket lets the kernel enforce the complete upstream peer.
    socket.connect(peer)?;
    loop {
        deadline.remaining()?;
        match socket.send(request) {
            Ok(size) if size == request.len() => break,
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "partial DNS datagram",
                ))
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                deadline.wait(socket.as_raw_fd(), libc::POLLOUT)?
            }
            Err(error) => return Err(error),
        }
    }
    let mut buffer = vec![0; u16::MAX as usize];
    loop {
        deadline.remaining()?;
        match socket.recv(&mut buffer) {
            Ok(size) => {
                let received_at = Instant::now();
                if question.validate_response(&buffer[..size]).is_ok() {
                    return Ok((buffer[..size].to_vec(), received_at));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                deadline.wait(socket.as_raw_fd(), libc::POLLIN)?
            }
            Err(error) => return Err(error),
        }
    }
}

fn tcp(
    request: &[u8],
    question: &Question,
    peer: SocketAddr,
    deadline: ExchangeDeadline<'_>,
) -> io::Result<(Vec<u8>, Instant)> {
    let mut socket = deadline.connect(peer)?;
    socket.set_nonblocking(true)?;
    let mut frame = Vec::with_capacity(request.len() + 2);
    frame.extend((request.len() as u16).to_be_bytes());
    frame.extend(request);
    let mut pending = frame.as_slice();
    while !pending.is_empty() {
        deadline.remaining()?;
        match socket.write(pending) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "DNS stream closed",
                ))
            }
            Ok(size) => pending = &pending[size..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                deadline.wait(socket.as_raw_fd(), libc::POLLOUT)?
            }
            Err(error) => return Err(error),
        }
    }
    let mut length = [0; 2];
    read_exact(&mut socket, &mut length, deadline)?;
    let mut response = vec![0; u16::from_be_bytes(length) as usize];
    read_exact(&mut socket, &mut response, deadline)?;
    let received_at = Instant::now();
    question.validate_response(&response).map_err(invalid_dns)?;
    Ok((response, received_at))
}

fn read_exact(
    socket: &mut TcpStream,
    mut pending: &mut [u8],
    deadline: ExchangeDeadline<'_>,
) -> io::Result<()> {
    while !pending.is_empty() {
        deadline.remaining()?;
        match socket.read(pending) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "DNS stream closed",
                ))
            }
            Ok(size) => pending = &mut pending[size..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                deadline.wait(socket.as_raw_fd(), libc::POLLIN)?
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
