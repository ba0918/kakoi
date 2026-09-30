//! Landlock: the kernel's restriction a process puts on itself and on everything it
//! starts, used by the "listed" modes (specification core-listed-mounts.md REQ-472 and
//! core-listed-commands.md). The system calls are made directly, as the seccomp filter is
//! assembled here rather than taken from a library.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

/// The first ABI version with the scope on abstract UNIX sockets.
pub const SCOPE_ABI: u32 = 6;

const CREATE_RULESET_VERSION: libc::c_uint = 1;
const SCOPE_ABSTRACT_UNIX_SOCKET: u64 = 1;
const ACCESS_FS_EXECUTE: u64 = 1;
const RULE_PATH_BENEATH: libc::c_uint = 1;

/// `struct landlock_ruleset_attr` as ABI 6 has it. A kernel of an older ABI rejects the
/// `scoped` field, which is why the scope is asked for only after the version is known.
#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
    handled_access_net: u64,
    scoped: u64,
}

/// The host's Landlock ABI version; none when the kernel has no Landlock or it is off.
pub fn abi_version() -> Option<u32> {
    // SAFETY: with a null attribute and a size of 0, the call only reads the flag.
    let version = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<RulesetAttr>(),
            0usize,
            CREATE_RULESET_VERSION,
        )
    };
    u32::try_from(version).ok().filter(|version| *version > 0)
}

/// Restricts the calling process, and whatever it starts, from connecting to an abstract
/// UNIX socket made outside this restriction. Makes only system calls, so it may run
/// between fork and exec.
pub fn scope_abstract_unix_sockets() -> io::Result<()> {
    let ruleset = create_ruleset(&RulesetAttr {
        handled_access_fs: 0,
        handled_access_net: 0,
        scoped: SCOPE_ABSTRACT_UNIX_SOCKET,
    })?;
    restrict_self(&ruleset)
}

/// `struct landlock_path_beneath_attr`, packed as the kernel declares it.
#[repr(C, packed)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}

/// Restricts the calling process, and whatever it starts, to executing only the files
/// `allowed` names (a directory stands for everything under it; each descriptor is
/// opened with `O_PATH`). Nothing but execution is restricted.
pub fn restrict_execution(allowed: &[OwnedFd]) -> io::Result<()> {
    let ruleset = create_ruleset(&RulesetAttr {
        handled_access_fs: ACCESS_FS_EXECUTE,
        handled_access_net: 0,
        scoped: 0,
    })?;
    for file in allowed {
        let rule = PathBeneathAttr {
            allowed_access: ACCESS_FS_EXECUTE,
            parent_fd: file.as_raw_fd(),
        };
        // SAFETY: `rule` is a valid `landlock_path_beneath_attr` for the whole call.
        let added = unsafe {
            libc::syscall(
                libc::SYS_landlock_add_rule,
                ruleset.as_raw_fd(),
                RULE_PATH_BENEATH,
                &rule as *const PathBeneathAttr,
                0u32,
            )
        };
        if added != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    restrict_self(&ruleset)
}

/// `path` opened as a path only, following links, so that a rule made from it lands on
/// what it leads to.
pub fn open_path(path: &std::path::Path) -> io::Result<OwnedFd> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH)
        .open(path)
        .map(OwnedFd::from)
}

fn create_ruleset(attr: &RulesetAttr) -> io::Result<OwnedFd> {
    // SAFETY: `attr` is a valid `landlock_ruleset_attr` of the size given.
    let fd = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            attr as *const RulesetAttr,
            std::mem::size_of::<RulesetAttr>(),
            0u32,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the call returned a new descriptor that nothing else holds.
    Ok(unsafe { OwnedFd::from_raw_fd(fd as i32) })
}

/// Enforces `ruleset` on the calling thread's process; a process that may gain privileges
/// through exec cannot be restricted, so that is given up first.
fn restrict_self(ruleset: &OwnedFd) -> io::Result<()> {
    // SAFETY: plain system calls on integer arguments and a descriptor this function holds.
    unsafe {
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
            return Err(io::Error::last_os_error());
        }
        if libc::syscall(libc::SYS_landlock_restrict_self, ruleset.as_raw_fd(), 0u32) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
