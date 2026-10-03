//! Application privilege boundary inside an already controlled network namespace.

use crate::{init, namespace::NetworkNamespace};
use kakoi_linux::{
    diagnostic::Diagnostic,
    launch::{self, BwrapCommand},
    plan::Plan,
};
use std::{os::fd::RawFd, time::Duration};

/// The supervisor must install and verify network enforcement before spawning the
/// returned command. This prepares the privilege boundary, not a network policy.
pub fn prepare(plan: &Plan, namespace: &NetworkNamespace) -> Result<BwrapCommand, Diagnostic> {
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    let target = launch::network_resolver_target(plan)?;
    let mut bwrap = launch::assemble_network(plan, uid, gid, target.as_deref(), None)?;
    enter(&mut bwrap, namespace)?;
    Ok(bwrap)
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
    let target = launch::network_resolver_target(plan)?;
    let mut bwrap = launch::assemble_network(
        plan,
        unsafe { libc::getuid() },
        unsafe { libc::getgid() },
        target.as_deref(),
        Some(&supervisor),
    )?;
    enter(&mut bwrap, namespace)?;
    Ok(bwrap)
}

fn enter(bwrap: &mut BwrapCommand, namespace: &NetworkNamespace) -> Result<(), Diagnostic> {
    namespace
        .enter_before_exec(&mut bwrap.command)
        .map_err(|error| {
            Diagnostic::bwrap(format!("prepare application network namespace: {error}"))
        })
}
