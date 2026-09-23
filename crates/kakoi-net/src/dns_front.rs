//! Bounded, nonblocking DNS framing. The supervisor calls `poll` between health
//! checks and submits work separately; client I/O never performs name resolution.

use crate::namespace::DnsSockets;
use hickory_proto::op::{Edns, Message};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};

const IO_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_FRAME: usize = u16::MAX as usize + 2;

pub struct IncomingQuery {
    pub wire: Vec<u8>,
    pub reply: ReplyToken,
}

pub struct ReplyToken(ReplyDestination);

enum ReplyDestination {
    Udp { peer: SocketAddr, limit: usize },
    Tcp(u64),
}

struct Peer {
    socket: TcpStream,
    input: Vec<u8>,
    output: Vec<u8>,
    written: usize,
    pending: bool,
    deadline: Instant,
}

pub struct DnsFront {
    sockets: DnsSockets,
    peers: BTreeMap<u64, Peer>,
    order: VecDeque<u64>,
    next_id: u64,
    max_connections: usize,
    resolution_timeout: Duration,
}

impl DnsFront {
    pub fn new(
        sockets: DnsSockets,
        max_connections: usize,
        resolution_timeout: Duration,
    ) -> io::Result<Self> {
        if !(1..=4096).contains(&max_connections)
            || resolution_timeout.is_zero()
            || resolution_timeout > Duration::from_secs(3600)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid DNS frontend limits",
            ));
        }
        sockets.udp.set_nonblocking(true)?;
        sockets.tcp.set_nonblocking(true)?;
        Ok(Self {
            sockets,
            peers: BTreeMap::new(),
            order: VecDeque::new(),
            next_id: 0,
            max_connections,
            resolution_timeout,
        })
    }

    /// At most 32 datagrams, 16 accepts and 64 round-robin stream steps per call.
    /// Idle/trickling frames and blocked replies have an absolute I/O deadline.
    pub fn poll(&mut self, now: Instant) -> io::Result<Vec<IncomingQuery>> {
        self.peers.retain(|_, peer| now < peer.deadline);
        self.order.retain(|id| self.peers.contains_key(id));
        let mut incoming = Vec::new();
        let mut datagram = [0; u16::MAX as usize];
        for _ in 0..32 {
            match self.sockets.udp.recv_from(&mut datagram) {
                Ok((size, peer)) => {
                    let wire = datagram[..size].to_vec();
                    let limit = Message::from_vec(&wire)
                        .map(|message| usize::from(message.max_payload()).clamp(512, 65507))
                        .unwrap_or(512);
                    incoming.push(IncomingQuery {
                        wire,
                        reply: ReplyToken(ReplyDestination::Udp { peer, limit }),
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        for _ in 0..16 {
            match self.sockets.tcp.accept() {
                Ok((socket, _)) => {
                    if self.peers.len() == self.max_connections {
                        continue;
                    }
                    socket.set_nonblocking(true)?;
                    let id = self.next_id;
                    self.next_id = id
                        .checked_add(1)
                        .ok_or_else(|| io::Error::other("DNS connection IDs exhausted"))?;
                    self.peers.insert(
                        id,
                        Peer {
                            socket,
                            input: Vec::new(),
                            output: Vec::new(),
                            written: 0,
                            pending: false,
                            deadline: now + IO_TIMEOUT,
                        },
                    );
                    self.order.push_back(id);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::ConnectionAborted
                    ) =>
                {
                    continue
                }
                Err(error) => return Err(error),
            }
        }
        for _ in 0..self.order.len().min(64) {
            let id = self.order.pop_front().expect("bounded by queue length");
            let peer = self.peers.get_mut(&id).expect("queue contains live peers");
            if peer.step(id, now, self.resolution_timeout, &mut incoming) {
                self.order.push_back(id);
            } else {
                self.peers.remove(&id);
            }
        }
        Ok(incoming)
    }

    /// `false` means the requester has gone away or a UDP response was dropped
    /// under backpressure. TCP replies are queued, never written synchronously.
    pub fn respond(&mut self, token: ReplyToken, wire: &[u8], now: Instant) -> io::Result<bool> {
        if wire.len() > u16::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "DNS response exceeds wire limit",
            ));
        }
        match token.0 {
            ReplyDestination::Udp { peer, limit } => {
                let truncated;
                let answer = if wire.len() > limit {
                    let mut message = Message::from_vec(wire)
                        .map_err(io::Error::other)?
                        .truncate();
                    let mut bytes = message.to_vec().map_err(io::Error::other)?;
                    if bytes.len() > limit {
                        // Large EDNS options must not defeat the client's advertised
                        // UDP limit; the full response remains available over TCP.
                        message.edns = Some(Edns::new());
                        bytes = message.to_vec().map_err(io::Error::other)?;
                    }
                    if bytes.len() > limit {
                        return Err(io::Error::other("truncated DNS response is too large"));
                    }
                    truncated = bytes;
                    truncated.as_slice()
                } else {
                    wire
                };
                match self.sockets.udp.send_to(answer, peer) {
                    Ok(size) => Ok(size == answer.len()),
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) =>
                    {
                        Ok(false)
                    }
                    Err(error) => Err(error),
                }
            }
            ReplyDestination::Tcp(id) => {
                let Some(peer) = self.peers.get_mut(&id) else {
                    return Ok(false);
                };
                if !peer.pending || now >= peer.deadline {
                    return Ok(false);
                }
                peer.output = Vec::with_capacity(wire.len() + 2);
                peer.output.extend((wire.len() as u16).to_be_bytes());
                peer.output.extend(wire);
                peer.written = 0;
                peer.deadline = now + IO_TIMEOUT;
                Ok(true)
            }
        }
    }
}

impl Peer {
    fn step(
        &mut self,
        id: u64,
        now: Instant,
        timeout: Duration,
        incoming: &mut Vec<IncomingQuery>,
    ) -> bool {
        if !self.output.is_empty() {
            let end = (self.written + 8192).min(self.output.len());
            match self.socket.write(&self.output[self.written..end]) {
                Ok(0) => return false,
                Ok(size) => self.written += size,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    return true
                }
                Err(_) => return false,
            }
            if self.written == self.output.len() {
                self.output.clear();
                self.pending = false;
                self.deadline = now + IO_TIMEOUT;
            }
            return true;
        }
        if self.pending {
            return true;
        }
        if self.input.len() < 2 || self.input.len() < self.frame_length() {
            let mut buffer = [0; 8192];
            let available = buffer.len().min(MAX_FRAME - self.input.len());
            match self.socket.read(&mut buffer[..available]) {
                Ok(0) => return false,
                Ok(size) => self.input.extend_from_slice(&buffer[..size]),
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    return true
                }
                Err(_) => return false,
            }
        }
        if self.input.len() >= 2 {
            let length = self.frame_length();
            if length == 2 {
                return false;
            }
            if self.input.len() >= length {
                let wire = self.input[2..length].to_vec();
                self.input.drain(..length);
                self.pending = true;
                // Keep the transport long enough to write a timeout SERVFAIL;
                // the resolution deadline itself remains unchanged.
                self.deadline = now + timeout + IO_TIMEOUT;
                incoming.push(IncomingQuery {
                    wire,
                    reply: ReplyToken(ReplyDestination::Tcp(id)),
                });
            }
        }
        true
    }

    fn frame_length(&self) -> usize {
        usize::from(u16::from_be_bytes([self.input[0], self.input[1]])) + 2
    }
}
