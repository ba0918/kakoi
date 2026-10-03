//! Policy parsing and command-rule evaluation without observing the host.

pub mod diagnostic;
pub mod guard;
pub mod layers;
pub mod network;
pub mod policy;
pub mod wildcard;

mod input;
pub use guard::{Commands, Examples, GuardRule, Position, Sequence};
pub use input::{EnvironmentPolicy, MountPolicy, NetworkPolicy, Policy, PolicyError, PolicyInput};
pub use network::{
    Allow, Destination, DnsPattern, DnsUpstream, FixedPublication, IpFamily, IpNetwork,
    LimitOverrides, Ports, Protocol,
};
pub use policy::{EnvMode, HideMounts, ListMode, NetworkMode, PolicyPath, Process, Scan, Variable};
