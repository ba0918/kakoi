//! Unstable implementation access for this repository's CLI and regression tests.
//!
//! Embedded consumers use RunRequest/PreparedRun/Running, not this connection layer.
pub use crate::cli_first_process::run_if_first_process;
pub use crate::cli_guard::run_if_guard;
pub use crate::{layers, planning};
pub use kakoi_linux::{
    executables, landlock, launch, mount_facts, mount_list, real_paths, scan, seccomp,
    secret_facts, shared_files, workspace_facts,
};
pub use kakoi_net as net;
pub use kakoi_plan::{
    command, command_limits, copies, environment, guard_placement, isolated_env, mounts, placement,
    variables,
};
pub use kakoi_policy::{diagnostic, guard, network, policy, wildcard};
pub mod plan {
    pub use kakoi_linux::bwrap_arguments::bwrap_arguments;
    pub use kakoi_plan::plan::*;
}

/// Writes `diagnostic` on standard error, one line, and gives its exit code (specification
/// section 13). Every role of this executable ends a failed run this way.
pub fn report(diagnostic: diagnostic::Diagnostic) -> std::process::ExitCode {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "{diagnostic}");
    std::process::ExitCode::from(diagnostic.exit_code() as u8)
}
