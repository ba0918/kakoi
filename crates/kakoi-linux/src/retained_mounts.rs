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

/// Shared files are created only for launch. Retain their selected inode before
/// handing the source to bwrap, just like the sources retained during prepare.
pub fn settle_shared(
    arguments: &[Argument],
    mounts: &mut Vec<RetainedMount>,
) -> io::Result<Vec<Argument>> {
    let settled = crate::shared_files::settle(arguments);
    for (index, window) in crate::bwrap_arguments::operations(arguments)? {
        if !matches!(window.get(1), Some(Argument::SharedFile { .. })) {
            continue;
        }
        for mut mount in retain(&settled[index..index + 3])? {
            mount.argument_index = index;
            mounts.push(mount);
        }
    }
    mounts.sort_by_key(RetainedMount::argument_index);
    Ok(settled)
}

/// Replace only the positions observed before the command boundary. A distinct
/// CLOEXEC descriptor is owned by this launch until bwrap consumes the reference.
pub(crate) fn descriptor_arguments(
    arguments: &[Argument],
    mounts: &[RetainedMount],
) -> io::Result<(Vec<Argument>, Vec<OwnedFd>)> {
    let mut arguments = arguments.to_vec();
    let mut descriptors = Vec::new();
    for mount in mounts {
        if mount.device {
            // Only the plan's own --dev-bind stays a path; init checks its result.
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
    Ok((arguments, descriptors))
}

pub fn retain(arguments: &[Argument]) -> io::Result<Vec<RetainedMount>> {
    let mut mounts = Vec::new();
    for (argument_index, arguments) in crate::bwrap_arguments::operations(arguments)? {
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
            // A device under --bind or --ro-bind stays nodev, as the CLI mounts it.
            device: option == "--dev-bind",
            argument_index,
            identity,
        });
    }
    Ok(mounts)
}

/// Remove a generated (non-retained) operation without reopening any source.
pub fn remove_generated_arguments(
    arguments: &mut Vec<Argument>,
    mounts: &mut [RetainedMount],
    range: std::ops::Range<usize>,
) -> io::Result<()> {
    if mounts
        .iter()
        .any(|mount| mount.argument_index < range.end && mount.argument_index + 3 > range.start)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot remove retained operation",
        ));
    }
    for mount in mounts {
        if mount.argument_index >= range.end {
            mount.argument_index -= range.len();
        }
    }
    arguments.drain(range);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn literals(words: &[&str]) -> Vec<Argument> {
        words
            .iter()
            .map(|word| Argument::Literal((*word).into()))
            .collect()
    }

    // A device named by a policy item stays nodev, as with the CLI's --ro-bind and --bind.
    #[test]
    fn policy_device_items_keep_nodev_descriptor_binds() {
        for (option, expected) in [("--ro-bind", "--ro-bind-fd"), ("--bind", "--bind-fd")] {
            let arguments = literals(&[option, "/dev/null", "/isolated/device"]);
            let mounts = retain(&arguments).unwrap();
            let (launched, descriptors) = descriptor_arguments(&arguments, &mounts).unwrap();
            assert_eq!(launched[0], Argument::Literal(expected.into()));
            assert_eq!(descriptors.len(), 1);
            assert!(!launched.contains(&Argument::Literal("--dev-bind".into())));
            assert!(!launched.contains(&Argument::Literal("--remount-ro".into())));
        }
    }

    #[test]
    fn planned_device_binds_stay_device_binds() {
        let arguments = literals(&["--dev-bind", "/dev/null", "/dev/net/tun"]);
        let mounts = retain(&arguments).unwrap();
        let (launched, descriptors) = descriptor_arguments(&arguments, &mounts).unwrap();
        assert_eq!(launched, arguments);
        assert!(descriptors.is_empty());
    }
}
