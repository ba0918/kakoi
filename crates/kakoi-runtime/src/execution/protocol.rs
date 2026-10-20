//! What the worker and the isolation's init send each other, and the check of a
//! generated file that both sides make.

use std::ffi::OsString;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;

use kakoi_linux::retained_mounts::Identity;
use serde::{Deserialize, Serialize};

use crate::{MainOutcome, RunOutcome, StartError};

pub(super) const INIT: &str = "/dev/kakoi-runtime/init";
pub(crate) const INIT_FD: &str = "KAKOI_RUNTIME_INIT_FD";

#[derive(Serialize, Deserialize)]
pub(super) struct Check {
    pub(super) path: Vec<u8>,
    pub(super) device: u64,
    pub(super) inode: u64,
    pub(super) kind: u32,
    pub(super) rdev: u64,
}
#[derive(Serialize, Deserialize)]
pub(super) struct DataCheck {
    pub(super) path: Vec<u8>,
    pub(super) readonly: bool,
}
#[derive(Serialize, Deserialize)]
pub(super) struct DataBatch {
    pub(super) offset: usize,
}
#[derive(Serialize, Deserialize)]
pub(super) struct InitRequest {
    pub(super) terminal_blocked: [bool; 4],
    pub(super) program: Vec<u8>,
    pub(super) argv0: Vec<u8>,
    pub(super) arguments: Vec<Vec<u8>>,
    pub(super) checks: Vec<Check>,
    pub(super) grace_ms: u64,
    pub(super) guards: Vec<Vec<u8>>,
    pub(super) allowed: Option<Vec<Vec<u8>>>,
    pub(super) data: Vec<DataCheck>,
}
#[derive(Serialize, Deserialize)]
pub(super) enum InitReply {
    Ready,
    Failed(StartError),
    Main(MainOutcome),
    Finished(RunOutcome),
}

pub(super) fn verify_data(check: &DataCheck, expected: OwnedFd) -> io::Result<()> {
    use std::io::Read;
    let path = PathBuf::from(OsString::from_vec(check.path.clone()));
    let mut actual = std::fs::File::open(&path)?;
    let identity = Identity::of_fd(&actual.try_clone()?.into())?;
    if identity.kind != libc::S_IFREG || Identity::of_path(&path)? != identity {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "generated file identity mismatch",
        ));
    }
    if check.readonly {
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        if unsafe { libc::fstatvfs(actual.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { stat.assume_init() }.f_flag & libc::ST_RDONLY == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "generated file is not read-only",
            ));
        }
    }
    let mut expected = std::fs::File::from(expected);
    if actual.metadata()?.len() != expected.metadata()?.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "generated file content changed",
        ));
    }
    let mut left = [0; 8192];
    let mut right = [0; 8192];
    loop {
        let size = expected.read(&mut right)?;
        if size == 0 {
            break;
        }
        actual.read_exact(&mut left[..size])?;
        if left[..size] != right[..size] {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "generated file content changed",
            ));
        }
    }
    Ok(())
}
