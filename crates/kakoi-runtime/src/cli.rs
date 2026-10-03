//! Unstable implementation access for this repository's CLI and regression tests.
//!
//! Embedded consumers use RunRequest/PreparedRun/Running, not this connection layer.
pub use crate::cli_first_process::run_if_first_process;
pub use crate::cli_guard::run_if_guard;
pub use crate::{layers, planning};
pub use kakoi_linux::{
    copy_facts, executables, landlock, launch, mount_facts, mount_list, regular_file, scan,
    seccomp, secret_facts, shared_files, workspace_facts,
};
pub use kakoi_net as net;
pub use kakoi_plan::{
    command, command_limits, copies, environment, guard_placement, isolated_env, listed, mounts,
    placement, variables,
};
pub use kakoi_policy::{diagnostic, guard, network, policy, wildcard};
pub mod plan {
    pub use kakoi_linux::bwrap_arguments::bwrap_arguments;
    pub use kakoi_plan::plan::*;
}
