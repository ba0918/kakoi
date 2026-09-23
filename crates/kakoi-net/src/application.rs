//! Application privilege boundary inside an already controlled network namespace.

use crate::{init, namespace::NetworkNamespace};
use kakoi_core::{
    diagnostic::Diagnostic,
    launch::{self, BwrapCommand},
    plan::{Argument, Plan},
};
use std::{os::fd::RawFd, time::Duration};

/// The supervisor must install and verify network enforcement before spawning the
/// returned command. This prepares the privilege boundary, not a network policy.
pub fn prepare(plan: &Plan, namespace: &NetworkNamespace) -> Result<BwrapCommand, Diagnostic> {
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    let mut effective = plan.clone();
    if plan.policy.network_mode == kakoi_core::policy::NetworkMode::Filtered {
        // bwrap 0.9 cannot mount data directly over an absolute symlink. Resolve
        // the host path in the executor, while keeping core planning pure.
        let target = std::fs::canonicalize("/etc/resolv.conf").map_err(|error| {
            Diagnostic::bwrap(format!(
                "locate application resolver configuration: {error}"
            ))
        })?;
        let index = effective
            .arguments
            .windows(3)
            .rposition(|args| {
                matches!(&args[0], Argument::Literal(value) if value == "--ro-bind-data")
                    && matches!(&args[1], Argument::CopiedFile(_))
                    && matches!(&args[2], Argument::Literal(value) if value == "/etc/resolv.conf")
            })
            .ok_or_else(|| {
                Diagnostic::bwrap("filtered plan lacks managed resolver configuration")
            })?;
        effective.arguments[index + 2] = Argument::Literal(target.into_os_string());
    }
    let mut bwrap = launch::assemble_for_network_executor(&effective, uid, gid)?;
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
    let mut supervised = plan.clone();
    supervise(&mut supervised.arguments, supervision)?;
    prepare(&supervised, namespace)
}

fn supervise(arguments: &mut Vec<Argument>, supervision: &Supervision) -> Result<(), Diagnostic> {
    let literal = |argument: &Argument, text: &str| matches!(argument, Argument::Literal(value) if value == text);
    let missing = || Diagnostic::bwrap("filtered plan lacks a command to supervise");
    let name = arguments
        .iter()
        .position(|argument| literal(argument, "--argv0"))
        .ok_or_else(missing)?;
    // The name given may itself be "--"; mount items and paths never are.
    let separator = name
        + 2
        + arguments[name + 2..]
            .iter()
            .position(|argument| literal(argument, "--"))
            .ok_or_else(missing)?;
    let Argument::Literal(given) = std::mem::replace(
        &mut arguments[name + 1],
        Argument::Literal(init::NAME.into()),
    ) else {
        return Err(missing());
    };
    let command = arguments.split_off(separator + 1);
    arguments.insert(separator, Argument::Literal("--as-pid-1".into()));
    arguments.extend(
        [
            format!("/proc/self/fd/{}", supervision.init),
            supervision.control.to_string(),
            supervision.init.to_string(),
            supervision.grace.as_millis().to_string(),
            u8::from(supervision.interrupt_ignored).to_string(),
        ]
        .map(|text| Argument::Literal(text.into())),
    );
    arguments.push(Argument::Literal(given));
    arguments.extend(command);
    Ok(())
}

fn enter(bwrap: &mut BwrapCommand, namespace: &NetworkNamespace) -> Result<(), Diagnostic> {
    namespace
        .enter_before_exec(&mut bwrap.command)
        .map_err(|error| {
            Diagnostic::bwrap(format!("prepare application network namespace: {error}"))
        })
}
