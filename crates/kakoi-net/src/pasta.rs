//! Foreground pasta ownership and its documented post-initialization PID handshake.

use crate::host::{HOST_LOOPBACK_V4, HOST_LOOPBACK_V6};
use crate::{child_output, health::TRANSIT_INTERFACE, namespace::NetworkNamespace};
use kakoi_core::network::{merge_publications, FixedPublication, IpFamily, Protocol};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

// The options kakoi passes to pasta by their long names. A pasta older than
// these names is told apart by its usage text (see `LONG_OPTIONS`).
const FOREGROUND: &str = "--foreground";
const PID: &str = "--pid";
const CONFIG_NET: &str = "--config-net";
const QUIET: &str = "--quiet";
const HOST_LO_TO_NS_LO: &str = "--host-lo-to-ns-lo";
const USERNS: &str = "--userns";
const NETNS: &str = "--netns";
const NO_MAP_GW: &str = "--no-map-gw";
const MAP_HOST_LOOPBACK: &str = "--map-host-loopback";

/// Every option either stage passes to pasta by a long name. The arguments
/// are built from these same names.
pub const LONG_OPTIONS: [&str; 9] = [
    FOREGROUND,
    PID,
    CONFIG_NET,
    QUIET,
    HOST_LO_TO_NS_LO,
    USERNS,
    NETNS,
    NO_MAP_GW,
    MAP_HOST_LOOPBACK,
];

#[derive(Debug, Clone, Copy)]
pub enum PastaStage {
    Outer,
    Inner,
}

pub struct Pasta {
    child: Child,
    readiness: ChildStdout,
    stderr: ChildStderr,
    diagnostics: Vec<u8>,
    // Keep the keeper unreaped, so the /proc/PID namespace paths cannot be reused.
    _target: Arc<NetworkNamespace>,
}

impl Pasta {
    pub fn start(
        executable: &Path,
        source: Option<&NetworkNamespace>,
        target: Arc<NetworkNamespace>,
        stage: PastaStage,
        publications: &[FixedPublication],
        timeout: Duration,
    ) -> io::Result<Self> {
        Self::start_controlled(
            executable,
            source,
            target,
            stage,
            publications,
            timeout,
            None,
        )
    }

    pub(crate) fn start_controlled(
        executable: &Path,
        source: Option<&NetworkNamespace>,
        target: Arc<NetworkNamespace>,
        stage: PastaStage,
        publications: &[FixedPublication],
        timeout: Duration,
        cancellation: Option<&crate::dns_workers::Cancellation>,
    ) -> io::Result<Self> {
        let publications = merge_publications(publications.iter().copied())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let mut command = match source {
            Some(namespace) => namespace.command(executable)?,
            None => {
                let mut command = Command::new(executable);
                // As for the namespace's commands: the terminal's Ctrl+C is not pasta's.
                command.process_group(0);
                command
            }
        };
        let target_pid = target.keeper_pid();
        command.args([
            FOREGROUND,
            PID,
            "/proc/self/fd/1",
            CONFIG_NET,
            // Only errors: the rest describes the host's network, which a
            // failed start would otherwise repeat in its diagnostic.
            QUIET,
            HOST_LO_TO_NS_LO,
            "-T",
            "none",
            "-U",
            "none",
        ]);
        command
            .arg(USERNS)
            .arg(format!("/proc/{target_pid}/ns/user"))
            .arg(NETNS)
            .arg(format!("/proc/{target_pid}/ns/net"));
        // Neither stage reads the gateway as the host: the inner stage would take it
        // to the middle namespace's loopback. Only the outer stage maps the
        // dedicated addresses to the host's loopback.
        command.arg(NO_MAP_GW);
        match stage {
            PastaStage::Outer => {
                command.args(["-I", TRANSIT_INTERFACE]);
                command
                    .arg(MAP_HOST_LOOPBACK)
                    .arg(HOST_LOOPBACK_V4.to_string())
                    .arg(MAP_HOST_LOOPBACK)
                    .arg(HOST_LOOPBACK_V6.to_string());
            }
            PastaStage::Inner => {
                command.args(["-i", TRANSIT_INTERFACE, "-I", "app0"]);
            }
        }
        for (protocol, flag) in [(Protocol::Tcp, "-t"), (Protocol::Udp, "-u")] {
            let matching: Vec<_> = publications
                .iter()
                .filter(|entry| entry.protocol == protocol)
                .collect();
            if matching.is_empty() {
                command.args([flag, "none"]);
            }
            for entry in matching {
                let address = match entry.family {
                    IpFamily::Ipv4 => "127.0.0.1",
                    IpFamily::Ipv6 => "::1",
                };
                let inside = match stage {
                    PastaStage::Outer => entry.host_port,
                    PastaStage::Inner => entry.port,
                };
                command
                    .arg(flag)
                    .arg(format!("{address}/{}:{inside}", entry.host_port));
            }
        }
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let readiness = child
            .stdout
            .take()
            .expect("piped stdout is present after spawn");
        let mut pasta = Self {
            stderr: child
                .stderr
                .take()
                .expect("piped stderr is present after spawn"),
            diagnostics: Vec::new(),
            child,
            readiness,
            _target: target,
        };
        for fd in [pasta.readiness.as_raw_fd(), pasta.stderr.as_raw_fd()] {
            child_output::nonblocking(fd)?;
        }
        if let Err(error) = pasta.await_ready(timeout, cancellation) {
            let _ = pasta.drain_diagnostics();
            return Err(child_output::with_diagnostic(error, &pasta.diagnostics));
        }
        Ok(pasta)
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn is_running(&mut self) -> io::Result<bool> {
        self.drain_diagnostics()?;
        if self.child.try_wait()?.is_some() {
            return Ok(false);
        }
        // try_wait observes exit, not SIGSTOP. Preserve stop notifications so
        // repeated polls cannot mistake a stopped forwarder for a healthy one.
        // SAFETY: all-zero bytes are a valid siginfo_t for waitid to fill.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                self.child.id(),
                &mut info,
                libc::WSTOPPED | libc::WCONTINUED | libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(info.si_code == 0 || info.si_code == libc::CLD_CONTINUED)
    }

    fn drain_diagnostics(&mut self) -> io::Result<()> {
        child_output::drain(&mut self.stderr, &mut self.diagnostics)
    }

    fn await_ready(
        &mut self,
        timeout: Duration,
        cancellation: Option<&crate::dns_workers::Cancellation>,
    ) -> io::Result<()> {
        let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "pasta startup timeout overflow",
            )
        })?;
        let mut received = Vec::new();
        loop {
            if cancellation.is_some_and(|cancel| cancel.is_cancelled()) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "pasta startup cancelled",
                ));
            }
            if !self.is_running()? {
                return Err(io::Error::other("pasta exited during startup"));
            }
            let mut buffer = [0; 32];
            match self.readiness.read(&mut buffer) {
                Ok(0) => return Err(io::Error::other("pasta closed its startup pipe")),
                Ok(count) => {
                    received.extend_from_slice(&buffer[..count]);
                    if received.contains(&b'\n') {
                        if received == format!("{}\n", self.pid()).as_bytes() {
                            return Ok(());
                        }
                        return Err(io::Error::other("invalid pasta startup PID"));
                    }
                    if received.len() > 16 {
                        return Err(io::Error::other("oversized pasta startup PID"));
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(error),
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "pasta startup timed out",
                ));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for Pasta {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
