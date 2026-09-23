//! Application privilege boundary inside an already controlled network namespace.

use crate::namespace::NetworkNamespace;
use kakoi_core::{
    diagnostic::Diagnostic,
    launch::{self, BwrapCommand},
    plan::{Argument, Plan},
};

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
    namespace
        .enter_before_exec(&mut bwrap.command)
        .map_err(|error| {
            Diagnostic::bwrap(format!("prepare application network namespace: {error}"))
        })?;
    Ok(bwrap)
}
