use super::{invalid_dns, random_id, ExchangeDeadline, ReceivedResponse};
use crate::dns::Question;
use rustls::{
    pki_types::{CertificateDer, ServerName},
    ClientConfig, ClientConnection, RootCertStore, StreamOwned,
};
use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    os::fd::AsRawFd,
    sync::Arc,
    time::Instant,
};

/// Immutable trust snapshot. Construct before entering application namespaces or
/// adopting application environment settings; keep it for the session lifetime.
#[derive(Clone)]
pub struct TlsClient {
    config: Arc<ClientConfig>,
}

impl TlsClient {
    pub fn from_host() -> io::Result<Self> {
        let loaded = rustls_native_certs::load_native_certs();
        if !loaded.errors.is_empty() || loaded.certs.is_empty() {
            return Err(io::Error::other(format!(
                "cannot load host CA certificates: {:?}",
                loaded.errors
            )));
        }
        Self::from_root_certificates(loaded.certs.into_iter().map(|cert| cert.as_ref().to_vec()))
    }

    /// Injection boundary for a trusted embedding controller and certificate
    /// tests. Product configuration uses `from_host`; no file-path or verification
    /// bypass is exposed through policy. All roots still use rustls verification.
    pub fn from_root_certificates(
        certificates: impl IntoIterator<Item = Vec<u8>>,
    ) -> io::Result<Self> {
        let mut roots = RootCertStore::empty();
        for certificate in certificates {
            roots
                .add(CertificateDer::from(certificate))
                .map_err(io::Error::other)?;
        }
        let config =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(io::Error::other)?
                .with_root_certificates(roots)
                .with_no_client_auth();
        Ok(Self {
            config: Arc::new(config),
        })
    }

    /// One DNS-over-TLS query, including connection and handshake in the supplied
    /// absolute deadline. Failure never invokes a plaintext transport.
    pub fn exchange(
        &self,
        wire: &[u8],
        peer: SocketAddr,
        name: &str,
        deadline: Instant,
    ) -> io::Result<ReceivedResponse> {
        self.exchange_controlled(wire, peer, name, ExchangeDeadline::new(deadline, None))
    }

    pub(super) fn exchange_controlled(
        &self,
        wire: &[u8],
        peer: SocketAddr,
        name: &str,
        deadline: ExchangeDeadline<'_>,
    ) -> io::Result<ReceivedResponse> {
        let original = Question::parse(wire).map_err(invalid_dns)?;
        let name = ServerName::try_from(name.to_owned()).map_err(io::Error::other)?;
        let mut request = wire.to_vec();
        random_id(&mut request[..2], deadline)?;
        let question = Question::parse(&request).map_err(invalid_dns)?;
        let socket = deadline.connect(peer)?;
        let connection =
            ClientConnection::new(self.config.clone(), name).map_err(io::Error::other)?;
        // rustls owns framing and cryptography. The socket adapter waits on
        // nonblocking I/O using the same deadline and cancellation throughout.
        let mut stream = StreamOwned::new(connection, DeadlineSocket { socket, deadline });
        stream.write_all(&(request.len() as u16).to_be_bytes())?;
        stream.write_all(&request)?;
        stream.flush()?;
        let mut length = [0; 2];
        stream.read_exact(&mut length)?;
        let mut answer = vec![0; u16::from_be_bytes(length) as usize];
        stream.read_exact(&mut answer)?;
        let received_at = Instant::now();
        deadline.remaining()?;
        question.validate_response(&answer).map_err(invalid_dns)?;
        answer[..2].copy_from_slice(&wire[..2]);
        Ok(ReceivedResponse {
            response: original.validate_response(&answer).map_err(invalid_dns)?,
            received_at,
        })
    }
}

struct DeadlineSocket<'a> {
    socket: TcpStream,
    deadline: ExchangeDeadline<'a>,
}

impl Read for DeadlineSocket<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            self.deadline.remaining()?;
            match self.socket.read(buffer) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    self.deadline.wait(self.socket.as_raw_fd(), libc::POLLIN)?
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }
}

impl Write for DeadlineSocket<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        loop {
            self.deadline.remaining()?;
            match self.socket.write(bytes) {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    self.deadline.wait(self.socket.as_raw_fd(), libc::POLLOUT)?
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.deadline.remaining()?;
        self.socket.flush()
    }
}
