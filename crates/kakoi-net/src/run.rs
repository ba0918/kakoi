//! The supervision loop of a filtered run: it keeps the network controlled while
//! the application runs and blocks it before any process is asked to end.

use crate::{
    exit::{exit_code, SAFETY_FAULT},
    notification::NotificationWriter,
    session::{Session, SessionState},
    supervisor::{Application, ApplicationEvent},
};
use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const INTERVAL: Duration = Duration::from_millis(20);
const CLOSURE: Duration = Duration::from_secs(2);
/// Draining and the last notifications must not hold the exit indefinitely.
const SETTLE: Duration = Duration::from_secs(2);

static TERMINATION: AtomicBool = AtomicBool::new(false);

extern "C" fn on_termination(_: libc::c_int) {
    TERMINATION.store(true, Ordering::SeqCst);
}

/// For the CLI: SIGTERM to this process becomes the termination request of
/// [`run`]. A Rust caller may pass a flag of its own instead.
pub fn termination_on_sigterm() -> io::Result<&'static AtomicBool> {
    // SAFETY: installs a handler that only stores to an atomic.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_termination as extern "C" fn(libc::c_int) as libc::sighandler_t;
        action.sa_flags = libc::SA_RESTART;
        if libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut()) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(&TERMINATION)
}

/// Blocking first; the processes are asked to end only once it is confirmed.
fn block(session: &mut Session, application: &mut Application, fault: &mut bool) -> bool {
    if session.close_until(Instant::now() + CLOSURE).is_err() {
        *fault = true;
        application.kill();
        false
    } else {
        true
    }
}

/// Runs until every process of the isolation has ended and returns the exit
/// code. `session` must be running and `application` started inside it.
pub fn run(
    session: &mut Session,
    application: &mut Application,
    writer: &mut NotificationWriter,
    terminate: &AtomicBool,
) -> u8 {
    let mut main = None;
    let mut terminated = false;
    let mut fault = false;
    let mut closed = false;
    let status = loop {
        let _ = session.poll();
        if session.state() == SessionState::Unsafe && !fault {
            // Neither enforcement nor blocking holds: no process may continue.
            fault = true;
            closed = true;
            application.kill();
        }
        if !closed && main.is_none() && terminate.load(Ordering::SeqCst) {
            terminated = true;
            closed = true;
            if block(session, application, &mut fault) && application.terminate().is_err() {
                application.kill();
            }
        }
        match application.poll() {
            Ok(Some(ApplicationEvent::MainExited(code))) => {
                main.get_or_insert(code);
                if !closed {
                    closed = true;
                    if block(session, application, &mut fault) && application.begin_grace().is_err()
                    {
                        application.kill();
                    }
                }
            }
            Ok(Some(ApplicationEvent::GraceInterrupted)) => {
                session.notice("grace interrupted; remaining processes were killed");
            }
            Ok(Some(ApplicationEvent::Finished(status))) => break status,
            Ok(None) => {}
            Err(error) => {
                session.notice(&format!("supervisor channel failed: {error}"));
                fault = true;
                application.kill();
            }
        }
        session.flush_notifications(writer);
        std::thread::sleep(INTERVAL);
    };
    if !closed && !block(session, application, &mut fault) {
        fault = true;
    }
    let settle = Instant::now() + SETTLE;
    while Instant::now() < settle && !(session.is_drained() && session.pending_notifications() == 0)
    {
        let _ = session.poll();
        session.flush_notifications(writer);
        std::thread::sleep(INTERVAL);
    }
    let main = main.unwrap_or_else(|| {
        status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .unwrap_or(SAFETY_FAULT)
    });
    exit_code(fault, terminated, main)
}
