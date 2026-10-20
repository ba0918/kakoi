//! Terminal signals handled only in a dedicated supervisor process.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicI32, AtomicU8, Ordering};

const TERMINAL: [i32; 4] = [libc::SIGHUP, libc::SIGINT, libc::SIGQUIT, libc::SIGTERM];
const INTERRUPT: u8 = 1;
const SHUTDOWN: u8 = 2;
static PENDING: AtomicU8 = AtomicU8::new(0);
static READER: AtomicI32 = AtomicI32::new(-1);
static WRITER: AtomicI32 = AtomicI32::new(-1);

extern "C" fn interrupt(signal: i32) {
    PENDING.fetch_or(
        if signal == libc::SIGINT {
            INTERRUPT
        } else {
            SHUTDOWN
        },
        Ordering::Relaxed,
    );
    // The pipe makes check/poll atomic with respect to an arriving signal.
    let fd = WRITER.load(Ordering::Relaxed);
    if fd >= 0 {
        // SAFETY: only async-signal-safe operations: errno is this thread's and is put
        // back, and write(2) of one byte from a static buffer to a descriptor the guard
        // keeps open while the handler is installed.
        unsafe {
            let errno = libc::__errno_location();
            let saved = *errno;
            libc::write(fd, b"x".as_ptr().cast(), 1);
            *errno = saved;
        }
    }
}

pub struct Guard {
    _reader: OwnedFd,
    _writer: OwnedFd,
}
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = block();
        WRITER.store(-1, Ordering::Relaxed);
        READER.store(-1, Ordering::Relaxed);
    }
}

pub fn descriptor() -> Option<i32> {
    let fd = READER.load(Ordering::Relaxed);
    (fd >= 0).then_some(fd)
}

/// The signals of `TERMINAL` whose entry in `included` is true.
fn set_of(included: [bool; 4]) -> libc::sigset_t {
    // SAFETY: an all-zero sigset_t is a valid value for sigemptyset to initialise, and
    // sigemptyset and sigaddset only write the set they are given.
    unsafe {
        let mut set = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        for (signal, included) in TERMINAL.into_iter().zip(included) {
            if included {
                libc::sigaddset(&mut set, signal);
            }
        }
        set
    }
}

pub fn blocked() -> io::Result<[bool; 4]> {
    // SAFETY: an all-zero sigset_t is valid; sigprocmask with no new set only writes
    // the current mask into it, and sigismember only reads it.
    let mut mask = unsafe { std::mem::zeroed() };
    if unsafe { libc::sigprocmask(libc::SIG_BLOCK, std::ptr::null(), &mut mask) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(TERMINAL.map(|signal| unsafe { libc::sigismember(&mask, signal) == 1 }))
}

/// Called after fork, before self-exec: initializers cannot kill the worker first.
pub fn block() -> io::Result<()> {
    // SAFETY: sigprocmask reads the set given and changes only this thread's mask.
    if unsafe { libc::sigprocmask(libc::SIG_BLOCK, &set_of([true; 4]), std::ptr::null_mut()) } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn restore(previously_blocked: [bool; 4]) -> io::Result<()> {
    let block = set_of(previously_blocked);
    let unblock = set_of(previously_blocked.map(|blocked| !blocked));
    // SAFETY: sigprocmask reads the sets given and changes only this thread's mask.
    if unsafe { libc::sigprocmask(libc::SIG_BLOCK, &block, std::ptr::null_mut()) } != 0
        || unsafe { libc::sigprocmask(libc::SIG_UNBLOCK, &unblock, std::ptr::null_mut()) } != 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Restore the caller's mask for these signals, but handle them in the worker.
/// Caught (not ignored) dispositions revert to default on child exec.
pub fn install(previously_blocked: [bool; 4]) -> io::Result<Guard> {
    let mut fds = [-1; 2];
    // SAFETY: pipe2 writes two descriptors into the array, which has room for them.
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_NONBLOCK | libc::O_CLOEXEC) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: both descriptors were just made and nothing else owns them.
    let guard = unsafe {
        Guard {
            _reader: OwnedFd::from_raw_fd(fds[0]),
            _writer: OwnedFd::from_raw_fd(fds[1]),
        }
    };
    READER.store(guard._reader.as_raw_fd(), Ordering::Relaxed);
    WRITER.store(guard._writer.as_raw_fd(), Ordering::Relaxed);
    for signal in TERMINAL {
        // SAFETY: an all-zero sigaction is valid (no flags, SIG_DFL); the handler set
        // here only does async-signal-safe work, and sigaction reads the struct given.
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = interrupt as *const () as usize;
        unsafe {
            libc::sigemptyset(&mut action.sa_mask);
        }
        if unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    let unblock = set_of(previously_blocked.map(|blocked| !blocked));
    // SAFETY: sigprocmask reads the set given and changes only this thread's mask.
    if unsafe { libc::sigprocmask(libc::SIG_UNBLOCK, &unblock, std::ptr::null_mut()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(guard)
}

pub fn pending() -> bool {
    PENDING.load(Ordering::Relaxed) != 0
}

/// During execution, Ctrl+C already reaches the target through the terminal's
/// foreground group. Do not forward it twice or replace it with a shutdown:
/// an application may cancel only its current operation and continue.
pub fn take_shutdown_requested() -> bool {
    if let Some(fd) = descriptor() {
        let mut buffer = [0u8; 64];
        // SAFETY: read(2) writes at most the buffer's length into it; the pipe is
        // non-blocking, so this drains it and stops.
        while unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) } > 0 {}
    }
    PENDING.swap(0, Ordering::Relaxed) & SHUTDOWN != 0
}
