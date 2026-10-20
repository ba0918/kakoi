//! Application privilege boundary inside an already controlled network namespace.

use crate::{init, namespace::NetworkNamespace};
use kakoi_linux::launch::{self, BwrapCommand};
use kakoi_plan::plan::Plan;
use kakoi_policy::diagnostic::Diagnostic;
use std::{os::fd::RawFd, time::Duration};

/// Connects the library's descriptor-mounted init to the same network boundary
/// as the CLI, without replacing its separately owned process supervisor.
pub fn prepare_retained(
    plan: &Plan,
    mounts: &[kakoi_linux::retained_mounts::RetainedMount],
    namespace: &NetworkNamespace,
) -> std::io::Result<BwrapCommand> {
    let (uid, gid) = identity();
    let mut bwrap = launch::assemble_retained_network(plan, mounts, uid, gid)?;
    namespace.enter_before_exec(&mut bwrap.command)?;
    Ok(bwrap)
}

/// The supervisor must install and verify network enforcement before spawning the
/// returned command. This prepares the privilege boundary, not a network policy.
pub fn prepare(plan: &Plan, namespace: &NetworkNamespace) -> Result<BwrapCommand, Diagnostic> {
    assemble(plan, namespace, None)
}

/// How the isolation's process 1 is started: descriptor numbers as they will be
/// inherited by bwrap, the shared grace, and the interrupt disposition to restore.
pub(crate) struct Supervision {
    pub init: RawFd,
    pub control: RawFd,
    pub grace: Duration,
    pub interrupt_ignored: bool,
}

/// Runs the plan's command under the supervisor as bwrap's process 1. The
/// command keeps the name it was given; the supervisor receives it as data.
pub(crate) fn prepare_supervised(
    plan: &Plan,
    namespace: &NetworkNamespace,
    supervision: &Supervision,
) -> Result<BwrapCommand, Diagnostic> {
    let supervisor = launch::ProcessSupervisor {
        argv0: init::NAME.into(),
        program: format!("/proc/self/fd/{}", supervision.init).into(),
        arguments: [
            supervision.control.to_string(),
            supervision.init.to_string(),
            supervision.grace.as_millis().to_string(),
            u8::from(supervision.interrupt_ignored).to_string(),
        ]
        .map(Into::into)
        .into(),
    };
    assemble(plan, namespace, Some(&supervisor))
}

/// The plan's bwrap command under the caller's identity, with the managed resolver in
/// place, entering `namespace` before it executes.
fn assemble(
    plan: &Plan,
    namespace: &NetworkNamespace,
    supervisor: Option<&launch::ProcessSupervisor>,
) -> Result<BwrapCommand, Diagnostic> {
    let (uid, gid) = identity();
    let target = launch::network_resolver_target(plan)?;
    let mut bwrap = launch::assemble_network(plan, uid, gid, target.as_deref(), supervisor)?;
    namespace
        .enter_before_exec(&mut bwrap.command)
        .map_err(|error| {
            Diagnostic::bwrap(format!("prepare application network namespace: {error}"))
        })?;
    Ok(bwrap)
}

/// The caller's user and group, which the isolation maps inside.
fn identity() -> (libc::uid_t, libc::gid_t) {
    // SAFETY: getuid and getgid only read the calling process's identity; they cannot fail.
    unsafe { (libc::getuid(), libc::getgid()) }
}
