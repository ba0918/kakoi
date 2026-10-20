//! Host observation and Linux operations; policy decisions live in lower layers.

#[cfg(not(target_arch = "x86_64"))]
compile_error!("kakoi supports only x86_64 (specification section 3)");

use kakoi_plan::{
    command, command_limits, copies, environment, guard_placement, isolated_env, listed, mounts,
    plan, variables,
};
use kakoi_policy::{diagnostic, layers, policy, wildcard};

pub mod bwrap_arguments;
pub mod command_location;
pub mod copy_facts;
pub mod executables;
pub mod helper_placement;
pub mod landlock;
pub mod launch;
pub mod mount_facts;
pub mod mount_list;
pub mod regular_file;
pub mod retained_mounts;
pub mod scan;
pub mod seccomp;
pub mod secret_facts;
pub mod shared_files;
pub mod supervisor_signals;
pub mod workspace_facts;
