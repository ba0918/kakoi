//! O_PATH references retained independently of path names during preparation.

use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use crate::plan::Argument;

pub struct RetainedMount {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub descriptor: OwnedFd,
    pub read_only: bool,
    pub device: bool,
    argument_index: usize,
    identity: Identity,
}
impl RetainedMount {
    pub fn argument_index(&self) -> usize {
        self.argument_index
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    pub device: u64,
    pub inode: u64,
    pub kind: u32,
    pub rdev: u64,
}

impl Identity {
    pub fn of_fd(fd: &OwnedFd) -> io::Result<Self> {
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        // SAFETY: fstat fills this initialized structure, without changing the FD.
        if unsafe { libc::fstat(fd.as_raw_fd(), &mut stat) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            device: stat.st_dev,
            inode: stat.st_ino,
            kind: stat.st_mode & libc::S_IFMT,
            rdev: stat.st_rdev,
        })
    }

    pub fn of_path(path: &std::path::Path) -> io::Result<Self> {
        Self::of_fd(&open_path(path)?)
    }
}

fn open_path(path: &std::path::Path) -> io::Result<OwnedFd> {
    let name = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in mount source"))?;
    // SAFETY: name is terminated and the returned descriptor is owned exactly once.
    let fd = unsafe { libc::open(name.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

pub fn recheck(mounts: &[RetainedMount]) -> io::Result<()> {
    for mount in mounts {
        let current = Identity::of_path(&mount.source)
            .map_err(|cause| io::Error::new(io::ErrorKind::InvalidData, cause))?;
        if current != mount.identity {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "mount source identity changed",
            ));
        }
    }
    Ok(())
}

/// Replace only the positions observed before the command boundary. A distinct
/// CLOEXEC descriptor is owned by this launch until bwrap consumes the reference.
pub(crate) fn descriptor_arguments(
    arguments: &[Argument],
    mounts: &[RetainedMount],
) -> io::Result<(Vec<Argument>, Vec<OwnedFd>)> {
    let mut arguments = arguments.to_vec();
    let mut descriptors = Vec::new();
    let mut remounts = Vec::new();
    for mount in mounts {
        if mount.device {
            // bind-fd uses nodev. Devices require dev-bind plus init's final check.
            arguments[mount.argument_index] = Argument::Literal("--dev-bind".into());
            if mount.read_only {
                remounts.push((mount.argument_index + 3, mount.destination.clone()));
            }
            continue;
        }
        let fd = mount.descriptor.try_clone()?;
        let option = if mount.read_only {
            "--ro-bind-fd"
        } else {
            "--bind-fd"
        };
        arguments[mount.argument_index] = Argument::Literal(option.into());
        arguments[mount.argument_index + 1] = Argument::Literal(fd.as_raw_fd().to_string().into());
        descriptors.push(fd);
    }
    // Insert backwards so the observed positions remain valid for every mount.
    for (index, destination) in remounts.into_iter().rev() {
        arguments.splice(
            index..index,
            [
                Argument::Literal("--remount-ro".into()),
                Argument::Literal(destination.into_os_string()),
            ],
        );
    }
    Ok((arguments, descriptors))
}

pub fn retain(arguments: &[Argument]) -> io::Result<Vec<RetainedMount>> {
    let mut mounts = Vec::new();
    for (argument_index, arguments) in arguments.windows(3).enumerate() {
        let Argument::Literal(option) = &arguments[0] else {
            continue;
        };
        if option != "--bind" && option != "--ro-bind" && option != "--dev-bind" {
            continue;
        }
        // Fixed shared files are prepared only at spawn, not at prepare.
        let Argument::Literal(source) = &arguments[1] else {
            continue;
        };
        let Argument::Literal(destination) = &arguments[2] else {
            continue;
        };
        let descriptor = open_path(std::path::Path::new(source))?;
        let identity = Identity::of_fd(&descriptor)?;
        mounts.push(RetainedMount {
            source: source.into(),
            destination: destination.into(),
            descriptor,
            read_only: option == "--ro-bind",
            device: option == "--dev-bind"
                || matches!(identity.kind, libc::S_IFCHR | libc::S_IFBLK),
            argument_index,
            identity,
        });
    }
    Ok(mounts)
}
