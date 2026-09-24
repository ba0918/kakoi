//! The supervisor that runs as process 1 inside the isolation (bwrap `--as-pid-1`).
//! It holds no network authority: it reports the main command's result, and only
//! after the outer controller has blocked the network does it give every remaining
//! process one shared grace. A forged message can only end the isolation sooner.

use std::{
    ffi::OsString,
    io,
    os::{fd::RawFd, unix::process::CommandExt},
    process::Command,
    sync::atomic::{AtomicI32, Ordering},
    time::{Duration, Instant},
};

pub(crate) const NAME: &str = "kakoi-net-init";
pub(crate) const BEGIN_GRACE: u8 = b'G';
pub(crate) const TERMINATE: u8 = b'T';
pub(crate) const MAIN_EXITED: u8 = b'E';
pub(crate) const GRACE_INTERRUPTED: u8 = b'I';

static SIGNAL_PIPE: AtomicI32 = AtomicI32::new(-1);

/// Call first in `main` of the executable handed to `Application::spawn`. Returns
/// when this process was not started as the isolation's supervisor.
pub fn run_if_requested() {
    let mut arguments = std::env::args_os();
    if arguments.next().as_deref() != Some(NAME.as_ref()) {
        return;
    }
    let code = match Arguments::parse(arguments.collect()) {
        Ok(arguments) => run(arguments),
        Err(error) => {
            eprintln!("kakoi: {NAME}: {error}");
            crate::exit::SAFETY_FAULT
        }
    };
    std::process::exit(code.into());
}

struct Arguments {
    control: RawFd,
    executable: RawFd,
    grace: Duration,
    interrupt_ignored: bool,
    given: OsString,
    path: OsString,
    rest: Vec<OsString>,
}

impl Arguments {
    fn parse(arguments: Vec<OsString>) -> Result<Self, String> {
        let mut arguments = arguments.into_iter();
        let mut number = |name: &str| -> Result<u64, String> {
            arguments
                .next()
                .and_then(|value| value.to_str()?.parse().ok())
                .ok_or_else(|| format!("missing or invalid {name}"))
        };
        let control = number("control descriptor")? as RawFd;
        let executable = number("executable descriptor")? as RawFd;
        let grace = Duration::from_millis(number("grace")?);
        let interrupt_ignored = number("interrupt disposition")? != 0;
        let given = arguments.next().ok_or("missing command name")?;
        let path = arguments.next().ok_or("missing command path")?;
        Ok(Self {
            control,
            executable,
            grace,
            interrupt_ignored,
            given,
            path,
            rest: arguments.collect(),
        })
    }
}

extern "C" fn on_signal(signal: libc::c_int) {
    // SAFETY: only async-signal-safe calls; errno is restored for the interrupted code.
    unsafe {
        let errno = *libc::__errno_location();
        let byte = signal as u8;
        libc::write(
            SIGNAL_PIPE.load(Ordering::Relaxed),
            (&byte as *const u8).cast(),
            1,
        );
        *libc::__errno_location() = errno;
    }
}

fn run(arguments: Arguments) -> u8 {
    let control = arguments.control;
    // SAFETY: plain descriptor and process-attribute calls on values this process
    // owns. SIGNAL_PIPE is stored before `on_signal` is installed, and the handler
    // only writes to it.
    let pipe = unsafe {
        libc::close(arguments.executable);
        libc::fcntl(control, libc::F_SETFD, libc::FD_CLOEXEC);
        // Keeps the application, which runs as the same user, out of this process.
        libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
        let mut pipe = [0; 2];
        if libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) != 0 {
            eprintln!("kakoi: {NAME}: {}", io::Error::last_os_error());
            return crate::exit::SAFETY_FAULT;
        }
        SIGNAL_PIPE.store(pipe[1], Ordering::Relaxed);
        for signal in [libc::SIGCHLD, libc::SIGINT] {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
            action.sa_flags = libc::SA_RESTART | libc::SA_NOCLDSTOP;
            libc::sigaction(signal, &action, std::ptr::null_mut());
        }
        pipe[0]
    };
    let mut state = State::default();
    let interrupt_ignored = arguments.interrupt_ignored;
    let mut command = Command::new(&arguments.path);
    command.arg0(&arguments.given).args(&arguments.rest);
    // SAFETY: the closure only changes signal state of the child before exec. Handled
    // signals return to their default at exec; an inherited ignore is restored here.
    unsafe {
        command.pre_exec(move || {
            let disposition = if interrupt_ignored {
                libc::SIG_IGN
            } else {
                libc::SIG_DFL
            };
            libc::signal(libc::SIGINT, disposition);
            let mut empty: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut empty);
            libc::sigprocmask(libc::SIG_SETMASK, &empty, std::ptr::null_mut());
            Ok(())
        })
    };
    match command.spawn() {
        Ok(child) => state.main = Some(child.id() as libc::pid_t),
        Err(error) => {
            eprintln!(
                "kakoi: {}: {error}",
                std::path::Path::new(&arguments.path).display()
            );
            let code = if error.kind() == io::ErrorKind::NotFound {
                127
            } else {
                126
            };
            state.result = Some(code);
            send(control, &[MAIN_EXITED, code]);
        }
    }
    loop {
        state.reap(control);
        if state.control_closed || state.deadline.is_some() {
            if state.result.is_some() && state.no_children {
                return state.result.unwrap_or(crate::exit::SAFETY_FAULT);
            }
            if state.control_closed && state.no_children {
                return crate::exit::SAFETY_FAULT;
            }
        }
        // Repeated while forcing: a process may fork between two broadcasts.
        if state.forced
            || state
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
        {
            state.force();
        }
        let timeout = match (state.forced, state.deadline) {
            (true, _) => 10,
            (false, Some(deadline)) => deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .clamp(1, 1000) as libc::c_int,
            (false, None) => 1000,
        };
        let mut fds = [
            libc::pollfd {
                fd: pipe,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: if state.control_closed { -1 } else { control },
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: both entries are valid pollfd values for the call.
        unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout) };
        let mut signals = [0u8; 64];
        // SAFETY: reads into a local buffer from this process's nonblocking pipe.
        let read = unsafe { libc::read(pipe, signals.as_mut_ptr().cast(), signals.len()) };
        if read > 0
            && signals[..read as usize].contains(&(libc::SIGINT as u8))
            && state.result.is_some()
            && state.deadline.is_some()
            && !state.forced
        {
            state.force();
            send(control, &[GRACE_INTERRUPTED]);
        }
        if fds[1].revents != 0 {
            state.receive(control, arguments.grace);
        }
    }
}

#[derive(Default)]
struct State {
    main: Option<libc::pid_t>,
    result: Option<u8>,
    deadline: Option<Instant>,
    forced: bool,
    no_children: bool,
    control_closed: bool,
}

impl State {
    fn reap(&mut self, control: RawFd) {
        loop {
            let mut status = 0;
            // SAFETY: waits for any child of this process without blocking.
            let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
            if pid < 0 {
                self.no_children = io::Error::last_os_error().raw_os_error() == Some(libc::ECHILD);
                return;
            }
            self.no_children = false;
            if pid == 0 {
                return;
            }
            if Some(pid) == self.main {
                let code = if libc::WIFSIGNALED(status) {
                    128 + libc::WTERMSIG(status) as u8
                } else {
                    libc::WEXITSTATUS(status) as u8
                };
                self.result = Some(code);
                send(control, &[MAIN_EXITED, code]);
            }
        }
    }

    fn receive(&mut self, control: RawFd, grace: Duration) {
        let mut message = [0u8; 1];
        // SAFETY: reads one message into a local buffer.
        let read = unsafe {
            libc::recv(
                control,
                message.as_mut_ptr().cast(),
                message.len(),
                libc::MSG_DONTWAIT,
            )
        };
        if read == 0 || (read < 0 && io::Error::last_os_error().kind() != io::ErrorKind::WouldBlock)
        {
            // The outer controller is gone; nothing may outlive its supervision.
            self.control_closed = true;
            self.force();
            return;
        }
        let begin = match message[0] {
            BEGIN_GRACE => self.result.is_some(),
            TERMINATE => true,
            _ => false,
        };
        if read == 1 && begin && self.deadline.is_none() {
            self.deadline = Some(Instant::now() + grace);
            // SAFETY: signals every other process of this PID namespace.
            unsafe {
                libc::kill(-1, libc::SIGTERM);
                libc::kill(-1, libc::SIGCONT);
            }
        }
    }

    fn force(&mut self) {
        self.forced = true;
        // SAFETY: as in `receive`.
        unsafe { libc::kill(-1, libc::SIGKILL) };
    }
}

fn send(control: RawFd, message: &[u8]) {
    // SAFETY: sends a local buffer; a lost controller is handled by `receive`.
    unsafe {
        libc::send(
            control,
            message.as_ptr().cast(),
            message.len(),
            libc::MSG_NOSIGNAL,
        )
    };
}
