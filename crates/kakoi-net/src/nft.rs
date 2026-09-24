//! Apply one nft transaction without letting a blocked pipe defeat its deadline.

use crate::child_output::{drain, nonblocking, with_diagnostic};
use crate::namespace::NetworkNamespace;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A failure does not prove that the kernel rejected the transaction: a timeout
/// can lose the success acknowledgement. The supervisor must then retain/restore
/// the closed gate or verify kernel state before permitting application traffic.
pub fn apply(
    namespace: &NetworkNamespace,
    executable: &Path,
    script: &str,
    deadline: Instant,
) -> io::Result<()> {
    run(namespace, executable, script, deadline, false, None).map(|_| ())
}

pub(crate) fn apply_cancellable(
    namespace: &NetworkNamespace,
    executable: &Path,
    script: &str,
    deadline: Instant,
    cancellation: &crate::dns_workers::Cancellation,
) -> io::Result<()> {
    run(
        namespace,
        executable,
        script,
        deadline,
        false,
        Some(cancellation),
    )
    .map(|_| ())
}

/// Collect JSON readback with bounded memory and the same subprocess deadline.
/// Read one inet table after a successful transaction.
pub fn inspect(
    namespace: &NetworkNamespace,
    executable: &Path,
    script: &str,
    deadline: Instant,
) -> io::Result<Vec<u8>> {
    run(namespace, executable, script, deadline, true, None)
}

fn read_output(reader: &mut impl Read, output: &mut Vec<u8>) -> io::Result<()> {
    let mut buffer = [0; 8192];
    for _ in 0..128 {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                if output.len() + count > 8 * 1024 * 1024 {
                    return Err(io::Error::other("nft readback exceeded 8 MiB"));
                }
                output.extend_from_slice(&buffer[..count]);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn run(
    namespace: &NetworkNamespace,
    executable: &Path,
    script: &str,
    deadline: Instant,
    capture: bool,
    cancellation: Option<&crate::dns_workers::Cancellation>,
) -> io::Result<Vec<u8>> {
    let check_cancel = || {
        if cancellation.is_some_and(|cancel| cancel.is_cancelled()) {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "nft command cancelled",
            ))
        } else {
            Ok(())
        }
    };
    check_cancel()?;
    if Instant::now() >= deadline {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "nft deadline already expired",
        ));
    }
    let mut process = OwnedChild(
        namespace
            .command(executable)?
            .args(if capture {
                vec!["-j", "list", "table", "inet", script]
            } else {
                vec!["-f", "-"]
            })
            .stdin(Stdio::piped())
            .stdout(if capture {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stderr(Stdio::piped())
            .spawn()?,
    );
    let mut input = process.0.stdin.take();
    let mut stdout = process.0.stdout.take();
    if let Some(reader) = &stdout {
        nonblocking(reader.as_raw_fd())?;
    }
    let mut output = Vec::new();
    let mut stderr = process
        .0
        .stderr
        .take()
        .expect("piped stderr exists after spawn");
    nonblocking(
        input
            .as_ref()
            .expect("piped stdin exists after spawn")
            .as_raw_fd(),
    )?;
    nonblocking(stderr.as_raw_fd())?;
    let input_bytes = if capture {
        b"".as_slice()
    } else {
        script.as_bytes()
    };
    let mut offset = 0;
    let mut diagnostic = Vec::new();
    let result = (|| loop {
        check_cancel()?;
        drain(&mut stderr, &mut diagnostic)?;
        if let Some(reader) = &mut stdout {
            read_output(reader, &mut output)?;
        }
        if let Some(status) = process.0.try_wait()? {
            drain(&mut stderr, &mut diagnostic)?;
            if let Some(reader) = &mut stdout {
                read_output(reader, &mut output)?;
            }
            return if status.success() && offset == input_bytes.len() {
                Ok(output)
            } else {
                Err(io::Error::other(format!("nft batch failed: {status}")))
            };
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "nft batch timed out",
            ));
        }
        if let Some(writer) = input.as_mut() {
            match writer.write(&input_bytes[offset..]) {
                Ok(0) if offset != input_bytes.len() => {
                    return Err(io::Error::new(io::ErrorKind::WriteZero, "nft input closed"))
                }
                Ok(count) => offset += count,
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(error),
            }
            if offset == input_bytes.len() {
                input.take();
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    })();
    result.map_err(|error| with_diagnostic(error, &diagnostic))
}
