//! Roles belong to copied execution images, not application names or argv.

use std::ffi::{OsStr, OsString};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use kakoi_linux::retained_mounts::Identity;
use kakoi_plan::guard_placement::{GuardTable, TableEntry, GUARD_TABLE};

const MAGIC: &[u8] = b"\0KAKOI-ROLE-V1\0";
const END: &[u8] = b"\0KAKOI-ROLE-END\0";
pub(crate) const PROBE: &str = "KAKOI_RUNTIME_PROBE_FD";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Init = 1,
    Guard = 2,
}

pub(crate) fn copy(role: Role) -> io::Result<Vec<u8>> {
    let mut bytes = std::fs::read("/proc/self/exe")?;
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(END);
    bytes.push(role as u8);
    Ok(bytes)
}

pub(crate) fn role() -> io::Result<Option<Role>> {
    let mut image = std::fs::File::open("/proc/self/exe")?;
    let size = MAGIC.len() + END.len() + 1;
    if image.metadata()?.len() < size as u64 {
        return Ok(None);
    }
    image.seek(SeekFrom::End(-(size as i64)))?;
    let mut trailer = vec![0; size];
    image.read_exact(&mut trailer)?;
    let header = &trailer[..MAGIC.len()] == MAGIC;
    let footer = &trailer[MAGIC.len()..size - 1] == END;
    if !header && !footer {
        return Ok(None);
    }
    if !header || !footer {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "corrupt image role framing",
        ));
    }
    match trailer[size - 1] {
        1 => Ok(Some(Role::Init)),
        2 => Ok(Some(Role::Guard)),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid image role",
        )),
    }
}

fn entry<'a>(table: &'a GuardTable, image: &std::fs::File) -> io::Result<&'a TableEntry> {
    let started = std::fs::read_link("/proc/self/exe")?;
    let identity = Identity::of_fd(&image.try_clone()?.into())?;
    let exact = table
        .entry(&started)
        .filter(|_| Identity::of_path(&started).ok() == Some(identity));
    if let Some(entry) = exact {
        return Ok(entry);
    }
    let stripped = started
        .as_os_str()
        .as_bytes()
        .strip_suffix(b" (deleted)")
        .map(|bytes| PathBuf::from(OsString::from_vec(bytes.to_vec())));
    stripped
        .as_deref()
        .and_then(|path| {
            table
                .entry(path)
                .filter(|_| Identity::of_path(path).ok() == Some(identity))
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "guard candidate identity mismatch",
            )
        })
}

fn guard() -> io::Result<ExitCode> {
    let image = std::fs::File::open("/proc/self/exe")?;
    let table = GuardTable::from_bytes(&std::fs::read(GUARD_TABLE)?)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid guard table"))?;
    let entry = entry(&table, &image)?;
    if let Some(number) = std::env::var_os(PROBE) {
        let fd = number
            .to_str()
            .and_then(|s| s.parse::<i32>().ok())
            .filter(|fd| *fd >= 3)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid probe control"))?;
        let control = crate::preparation::inherited_socket(fd).map_err(io::Error::other)?;
        crate::ipc::send(&control, &true, &[])?;
        return Ok(ExitCode::SUCCESS);
    }
    let mut arguments = std::env::args_os();
    let name = arguments.next().unwrap_or_default();
    let arguments: Vec<_> = arguments.collect();
    let environment: Vec<_> = std::env::vars_os().map(|(name, _)| name).collect();
    if let Some(denial) = kakoi_policy::guard::evaluate(&entry.rules, &arguments, &environment) {
        eprintln!("{} {}: {}", denial.program, denial.matched, denial.reason);
        return Ok(ExitCode::from(126));
    }
    let mut command = Command::new(Path::new(OsStr::from_bytes(&entry.execute)));
    crate::preparation::remove_helper_environment(&mut command);
    command.arg0(name).args(arguments);
    unsafe {
        command.pre_exec(|| kakoi_linux::launch::inherit_only(&[]));
    }
    Err(command.exec())
}

pub(crate) fn dispatch_guard() -> ExitCode {
    match guard() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("command guard: {error}");
            ExitCode::from(126)
        }
    }
}

pub(crate) fn probe(path: &Path) -> io::Result<()> {
    let (parent, child) = std::os::unix::net::UnixStream::pair()?;
    let fd = child.as_raw_fd();
    let mut command = Command::new(path);
    crate::preparation::remove_helper_environment(&mut command);
    command.env(PROBE, fd.to_string());
    unsafe {
        command.pre_exec(move || kakoi_linux::launch::inherit_only(&[fd]));
    }
    let mut process = command.spawn()?;
    drop(child);
    let (result, status) = loop {
        let mut poll = libc::pollfd {
            fd: parent.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut poll, 1, 20) };
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            let _ = process.kill();
            let _ = process.wait();
            return Err(error);
        }
        if ready > 0 {
            break (crate::ipc::recv::<bool>(&parent), process.wait()?);
        }
        if let Some(status) = process.try_wait()? {
            // An initializer's descendant can retain the socket after the image dies.
            // Neither that retained endpoint nor error-pipe EOF proves dispatch.
            break (
                Err(io::Error::other("probe exited without dispatch")),
                status,
            );
        }
    };
    match result {
        Ok((true, fds)) if fds.is_empty() && status.success() => Ok(()),
        _ => Err(io::Error::other("guard did not confirm dispatch")),
    }
}
