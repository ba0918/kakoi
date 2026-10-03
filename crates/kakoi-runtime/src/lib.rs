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

pub mod config;
mod input;
pub use input::{HostContext, InputError};
pub use kakoi_policy::{
    Allow, Commands, Destination, DnsPattern, DnsUpstream, EnvMode, EnvironmentPolicy, Examples,
    FixedPublication, GuardRule, HideMounts, IpFamily, IpNetwork, LimitOverrides, ListMode,
    MountPolicy, NetworkMode, NetworkPolicy, Policy, PolicyError, PolicyInput, PolicyPath, Ports,
    Position, Process, Protocol, Scan, Sequence, Variable,
};
