//! Isolation orchestration, with host observation delegated to Linux operations.

use kakoi_linux::{
    copy_facts, executables, landlock, mount_facts, regular_file, secret_facts, shared_files,
    workspace_facts,
};
use kakoi_plan::{
    command, command_limits, environment, guard_placement, listed, mounts, placement, plan,
    variables,
};
use kakoi_policy::{diagnostic, policy};

pub mod layers;
pub mod planning;
