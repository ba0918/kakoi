//! The helper roles this executable is started in: the worker that prepares and runs one
//! isolation for its parent, the isolation's init and the command guards, or the caller's
//! own application.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::net::{UnixDatagram, UnixStream};
use std::path::PathBuf;
use std::process::ExitCode;

use super::description::DescriptionInput;
use super::error::{error, io_error, ErrorKind, Phase, PrepareError};
use super::{path, Hello, IoInput, RequestInput, Response, EVENTS, OWNER, RESULT, VERSION, WORKER};
use crate::{ipc, wire_policy, HostContext};

#[derive(Debug)]
pub enum Dispatch {
    Application,
    Completed(ExitCode),
}

pub type DispatchError = PrepareError;

fn descriptor_number(name: &str) -> Result<i32, PrepareError> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|fd| *fd >= 3)
        .ok_or_else(|| {
            error(
                Phase::Worker,
                ErrorKind::Protocol,
                "missing or invalid helper descriptor",
            )
        })
}

pub(crate) fn inherited_socket(fd: i32) -> Result<UnixStream, PrepareError> {
    // Validate the inherited capability before owning it.
    let mut ty: libc::c_int = 0;
    let mut size = std::mem::size_of_val(&ty) as libc::socklen_t;
    // SAFETY: pointers are valid, getsockopt only writes within their stated sizes.
    if unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            (&mut ty as *mut libc::c_int).cast(),
            &mut size,
        )
    } < 0
        || ty != libc::SOCK_STREAM
    {
        return Err(error(
            Phase::Worker,
            ErrorKind::Protocol,
            "helper descriptor is not a stream socket",
        ));
    }
    // SAFETY: ownership is transferred from the re-executed parent's descriptor table.
    let socket = unsafe { UnixStream::from_raw_fd(fd) };
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(io_error(Phase::Worker, io::Error::last_os_error()));
    }
    Ok(socket)
}

pub fn dispatch_helper() -> Result<Dispatch, DispatchError> {
    if let Some(dispatch) = crate::execution::dispatch_image()? {
        return Ok(dispatch);
    }
    if std::env::var_os(WORKER).is_none()
        && std::env::var_os(OWNER).is_none()
        && std::env::var_os(RESULT).is_none()
        && std::env::var_os(EVENTS).is_none()
    {
        return Ok(Dispatch::Application);
    }
    let control_fd = descriptor_number(WORKER)?;
    let owner_fd = descriptor_number(OWNER)?;
    let result_fd = descriptor_number(RESULT)?;
    let event_fd = descriptor_number(EVENTS)?;
    let channel_fds = [control_fd, owner_fd, result_fd, event_fd];
    if channel_fds
        .iter()
        .enumerate()
        .any(|(i, fd)| channel_fds[..i].contains(fd))
    {
        return Err(error(
            Phase::Worker,
            ErrorKind::Protocol,
            "helper channels must be distinct",
        ));
    }
    let control = inherited_socket(control_fd)?;
    let owner = inherited_socket(owner_fd)?;
    let result_channel = inherited_socket(result_fd)?;
    let mut ty: libc::c_int = 0;
    let mut size = std::mem::size_of_val(&ty) as libc::socklen_t;
    // SAFETY: getsockopt writes into the initialized type and size only.
    if unsafe {
        libc::getsockopt(
            event_fd,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            (&mut ty as *mut libc::c_int).cast(),
            &mut size,
        )
    } < 0
        || ty != libc::SOCK_DGRAM
    {
        return Err(error(
            Phase::Worker,
            ErrorKind::Protocol,
            "invalid event descriptor",
        ));
    }
    // SAFETY: the validated, distinct descriptor is inherited exclusively by this worker.
    let event_channel = unsafe { UnixDatagram::from_raw_fd(event_fd) };
    unsafe {
        libc::fcntl(event_fd, libc::F_SETFD, libc::FD_CLOEXEC);
    }
    std::env::remove_var(WORKER);
    std::env::remove_var(OWNER);
    std::env::remove_var(RESULT);
    std::env::remove_var(EVENTS);
    ipc::send(&control, &Hello { version: VERSION }, &[])
        .map_err(|cause| io_error(Phase::Worker, cause))?;
    let result = prepare_worker(&control, &owner, &result_channel, event_channel);
    match result {
        Ok(()) => Ok(Dispatch::Completed(ExitCode::SUCCESS)),
        Err(error) => {
            let _ = ipc::send(&control, &Response::Failed(error.clone()), &[]);
            Err(error)
        }
    }
}

fn prepare_worker(
    control: &UnixStream,
    owner: &UnixStream,
    result_channel: &UnixStream,
    event_channel: UnixDatagram,
) -> Result<(), PrepareError> {
    let (input, descriptors): (RequestInput, _) = ipc::recv_watched(control, &[owner.as_raw_fd()])
        .map_err(|cause| io_error(Phase::Worker, cause))?;
    if input.version != VERSION {
        return Err(error(
            Phase::Worker,
            ErrorKind::Protocol,
            "request version mismatch",
        ));
    }
    let _signals = kakoi_linux::supervisor_signals::install(input.terminal_blocked)
        .map_err(|cause| io_error(Phase::Worker, cause))?;
    let expected = input
        .stdio
        .iter()
        .filter(|entry| matches!(entry, IoInput::Descriptor(_)))
        .count();
    if descriptors.len() != expected {
        return Err(error(
            Phase::Worker,
            ErrorKind::Protocol,
            "stdio descriptor count mismatch",
        ));
    }
    for (index, entry) in input
        .stdio
        .iter()
        .filter_map(|entry| {
            if let IoInput::Descriptor(index) = entry {
                Some(*index)
            } else {
                None
            }
        })
        .enumerate()
    {
        if index != entry {
            return Err(error(
                Phase::Worker,
                ErrorKind::Protocol,
                "stdio descriptor index mismatch",
            ));
        }
    }
    let env: BTreeMap<_, _> = input
        .env
        .into_iter()
        .map(|(key, value)| (OsString::from_vec(key), OsString::from_vec(value)))
        .collect();
    let context = HostContext::new(path(input.cwd), env)
        .map_err(|cause| error(Phase::Input, ErrorKind::InvalidInput, cause.to_string()))?;
    if input.program.contains(&0)
        || input.arguments.iter().any(|argument| argument.contains(&0))
        || input
            .workspace
            .as_ref()
            .is_some_and(|workspace| workspace.contains(&0))
    {
        return Err(error(
            Phase::Input,
            ErrorKind::InvalidInput,
            "NUL in request",
        ));
    }
    let policy = wire_policy::decode(input.policy)
        .map_err(|cause| error(Phase::Input, ErrorKind::InvalidInput, cause))?;
    std::env::set_current_dir(context.cwd()).map_err(|cause| io_error(Phase::Planning, cause))?;
    kakoi_linux::launch::raise_open_file_limit();
    let mut command = vec![OsString::from_vec(input.program)];
    command.extend(input.arguments.into_iter().map(OsString::from_vec));
    let nested = std::path::Path::new(crate::plan::NESTING_MARK).exists();
    let outer_guard = std::path::Path::new(crate::guard_placement::GUARD_ROOT).exists();
    let request = crate::planning::Request {
        layers: crate::layers::LayerSelection {
            profile: String::new(),
            policy_file: None,
            rw: Vec::new(),
            hide: Vec::new(),
        },
        workspace: input.workspace.map(path),
        command,
        current_dir: context.cwd().into(),
        host: context.environment().clone(),
        executable: Some(PathBuf::from("/proc/self/exe")),
        nested,
        applied: true,
        outer_guard,
        outer_table: if outer_guard {
            std::fs::read(crate::guard_placement::GUARD_TABLE).ok()
        } else {
            None
        },
        landlock_abi: kakoi_linux::landlock::abi_version(),
    };
    let mut plan =
        crate::planning::plan_with_policy(&request, policy.source_layers(), policy.as_merged())
            .map_err(|cause| {
                let io_cause = cause.io_cause();
                let kind = if io_cause.is_some() {
                    ErrorKind::Io
                } else if cause.kind() == crate::diagnostic::Kind::Bwrap {
                    ErrorKind::UnsupportedEnvironment
                } else {
                    ErrorKind::InvalidInput
                };
                let mut failure = error(Phase::Planning, kind, cause.to_string());
                failure.diagnostics[0].os_error = io_cause.and_then(|cause| cause.os_error);
                failure
            })?;
    let guard_image = if plan.guards.table.entries.is_empty() {
        None
    } else {
        Some(kakoi_plan::copies::FileContent::new(
            crate::helper_image::copy(crate::helper_image::Role::Guard)
                .map_err(|cause| io_error(Phase::Retention, cause))?,
        ))
    };
    kakoi_linux::helper_placement::place(&mut plan, guard_image)
        .map_err(|cause| io_error(Phase::Retention, cause))?;
    let separator = plan.launch_layout.command_separator.ok_or_else(|| {
        error(
            Phase::Retention,
            ErrorKind::Protocol,
            "missing command boundary",
        )
    })?;
    let mounts = kakoi_linux::retained_mounts::retain(&plan.arguments[..separator])
        .map_err(|cause| io_error(Phase::Retention, cause))?;
    let sources = crate::execution::source_checks(&plan, &mounts, context.cwd())
        .map_err(|cause| io_error(Phase::Retention, cause))?;
    ipc::send_watched(
        control,
        &Response::Prepared(Box::new(DescriptionInput::from_plan(&plan, context.cwd()))),
        &[],
        &[owner.as_raw_fd()],
    )
    .map_err(|cause| io_error(Phase::Worker, cause))?;
    // The command and network are not started at preparation. Resources stay here
    // until the sole owner's endpoint closes; observer/reaper state cannot keep it alive.
    let mut poll = [
        libc::pollfd {
            fd: owner.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: control.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: kakoi_linux::supervisor_signals::descriptor().unwrap(),
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    loop {
        // SAFETY: poll only writes the initialized entries in this array.
        let result = unsafe { libc::poll(poll.as_mut_ptr(), poll.len() as libc::nfds_t, -1) };
        if result < 0 {
            let cause = io::Error::last_os_error();
            if cause.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(io_error(Phase::Worker, cause));
        }
        if poll[0].revents != 0 {
            return Ok(());
        }
        if poll[2].revents != 0 {
            return Err(io_error(
                Phase::Worker,
                io::Error::new(
                    io::ErrorKind::Interrupted,
                    "supervisor received a terminal signal",
                ),
            ));
        }
        if poll[1].revents != 0 {
            let (command, fds): (crate::running::Control, _) =
                ipc::recv_watched(control, &[owner.as_raw_fd()])
                    .map_err(|cause| io_error(Phase::Worker, cause))?;
            if !fds.is_empty() {
                return Err(error(
                    Phase::Worker,
                    ErrorKind::Protocol,
                    "unexpected control descriptors",
                ));
            }
            if matches!(command, crate::running::Control::Start) {
                return crate::execution::run_worker(
                    plan,
                    mounts,
                    sources,
                    input.stdio,
                    descriptors,
                    context.environment(),
                    crate::execution::Channels {
                        control,
                        owner,
                        result: result_channel,
                        events: event_channel,
                    },
                );
            }
        }
    }
}
