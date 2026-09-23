//! Foreground pasta ownership and its documented post-initialization PID handshake.

use crate::{health::TRANSIT_INTERFACE, namespace::NetworkNamespace};
use kakoi_core::network::{merge_publications, FixedPublication, IpFamily, Protocol};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
            None => Command::new(executable),
        };
        let target_pid = target.keeper_pid();
        command.args([
            "--foreground",
            "--pid",
            "/proc/self/fd/1",
            "--config-net",
            "--host-lo-to-ns-lo",
            "-T",
            "none",
            "-U",
            "none",
        ]);
        command
            .arg("--userns")
            .arg(format!("/proc/{target_pid}/ns/user"))
            .arg("--netns")
            .arg(format!("/proc/{target_pid}/ns/net"));
        match stage {
            PastaStage::Outer => {
                command.args(["-I", TRANSIT_INTERFACE]);
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
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        if let Err(error) = pasta.await_ready(timeout, cancellation) {
            let _ = pasta.drain_diagnostics();
            let diagnostic = kakoi_core::diagnostic::escape_control(&String::from_utf8_lossy(
                &pasta.diagnostics,
            ));
            return Err(io::Error::new(
                error.kind(),
                format!("{error}: {diagnostic}"),
            ));
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
        let mut buffer = [0; 1024];
        // Bound per-poll work as well as retained memory; drain a full Linux pipe
        // so a tool that just exited does not lose the end of its diagnostic.
        for _ in 0..1024 {
            match self.stderr.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    self.diagnostics.extend_from_slice(&buffer[..count]);
                    let excess = self.diagnostics.len().saturating_sub(8192);
                    self.diagnostics.drain(..excess);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(())
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
