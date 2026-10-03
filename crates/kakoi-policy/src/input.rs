//! Explicit-mode Rust input, normalized into the same schema used by TOML.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use crate::diagnostic::Diagnostic;
use crate::guard::Commands;
use crate::layers::{merge, Layer, LayerOrigin};
use crate::network::{Allow, DnsUpstream, FixedPublication, LimitOverrides};
use crate::policy::{
    parse_policy, validate_policy, Env, EnvMode, Git, HideMounts, ListMode, Mounts, Network,
    NetworkMode, PolicyFile, PolicyPath, Process, Scan,
};

/// A mount mode must be selected before a Rust input can be built.
#[derive(Clone, PartialEq, Eq)]
pub struct MountPolicy {
    mode: ListMode,
    pub system: Option<bool>,
    pub rw: Vec<PolicyPath>,
    pub rw_file: Vec<PolicyPath>,
    pub rw_copy: Vec<PolicyPath>,
    pub ro: Vec<PolicyPath>,
    pub hide: Vec<PolicyPath>,
    pub scan: Vec<Scan>,
    pub hide_mounts: Vec<HideMounts>,
}

impl MountPolicy {
    pub fn new(mode: ListMode) -> Self {
        Self {
            mode,
            system: None,
            rw: Vec::new(),
            rw_file: Vec::new(),
            rw_copy: Vec::new(),
            ro: Vec::new(),
            hide: Vec::new(),
            scan: Vec::new(),
            hide_mounts: Vec::new(),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct NetworkPolicy {
    mode: NetworkMode,
    pub publish: Vec<FixedPublication>,
    pub allow: Vec<Allow>,
    pub limits: LimitOverrides,
    pub dns_upstream: Vec<DnsUpstream>,
    pub allow_nested_filtered: Option<bool>,
}

impl NetworkPolicy {
    pub fn new(mode: NetworkMode) -> Self {
        Self {
            mode,
            publish: Vec::new(),
            allow: Vec::new(),
            limits: LimitOverrides::default(),
            dns_upstream: Vec::new(),
            allow_nested_filtered: None,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct EnvironmentPolicy {
    mode: EnvMode,
    pub pass: Vec<String>,
    pub set: BTreeMap<String, String>,
    pub unset: Vec<String>,
    pub path_prepend: Vec<PolicyPath>,
}

impl EnvironmentPolicy {
    pub fn new(mode: EnvMode) -> Self {
        Self {
            mode,
            pass: Vec::new(),
            set: BTreeMap::new(),
            unset: Vec::new(),
            path_prepend: Vec::new(),
        }
    }
}

/// Editable input. Only TOML may omit the three primary modes.
#[derive(Clone, PartialEq, Eq)]
pub struct PolicyInput {
    pub mounts: MountPolicy,
    pub network: NetworkPolicy,
    pub environment: EnvironmentPolicy,
    pub process: Process,
    pub secrets: BTreeMap<String, PolicyPath>,
    pub instead_of: BTreeMap<String, String>,
    pub commands: Commands,
}

impl PolicyInput {
    pub fn new(
        mounts: MountPolicy,
        network: NetworkPolicy,
        environment: EnvironmentPolicy,
    ) -> Self {
        Self {
            mounts,
            network,
            environment,
            process: Process::default(),
            secrets: BTreeMap::new(),
            instead_of: BTreeMap::new(),
            commands: Commands::default(),
        }
    }
}

impl From<PolicyInput> for PolicyFile {
    fn from(input: PolicyInput) -> Self {
        let mounts = input.mounts;
        let network = input.network;
        let env = input.environment;
        Self {
            mounts: Mounts {
                mode: Some(mounts.mode),
                system: mounts.system,
                rw: mounts.rw,
                rw_file: mounts.rw_file,
                rw_copy: mounts.rw_copy,
                ro: mounts.ro,
                hide: mounts.hide,
                scan: mounts.scan,
                hide_mounts: mounts.hide_mounts,
            },
            network: Network {
                mode: Some(network.mode),
                settings_present: !network.publish.is_empty()
                    || !network.allow.is_empty()
                    || !network.dns_upstream.is_empty()
                    || network.limits.is_present(),
                publish: network.publish,
                allow: network.allow,
                limits: network.limits,
                dns_upstream: network.dns_upstream,
                allow_nested_filtered: network.allow_nested_filtered,
            },
            env: Env {
                mode: Some(env.mode),
                pass: env.pass,
                set: env.set,
                unset: env.unset,
                path_prepend: env.path_prepend,
            },
            process: input.process,
            secrets: input.secrets,
            git: Git {
                instead_of: input.instead_of,
            },
            commands: input.commands,
        }
    }
}

/// A validated policy. No mutable fields or unchecked deserialization are exposed.
#[derive(Clone, PartialEq, Eq)]
pub struct Policy(crate::layers::Policy);

pub type PolicyError = Diagnostic;

impl Policy {
    pub fn from_toml(source: &str) -> Result<Self, PolicyError> {
        Self::from_file(parse_policy(source, Path::new("<memory>"))?)
    }

    pub fn validate(input: PolicyInput) -> Result<Self, PolicyError> {
        let mut file = PolicyFile::from(input);
        validate_policy(&mut file, Path::new("<memory>"))?;
        Self::from_file(file)
    }

    fn from_file(file: PolicyFile) -> Result<Self, PolicyError> {
        Self::from_layers(&[Layer {
            origin: LayerOrigin::CommandLine,
            policy: file,
        }])
    }

    /// Shared entry for explicit configuration loading by the runtime.
    #[doc(hidden)]
    pub fn from_layers(layers: &[Layer]) -> Result<Self, PolicyError> {
        let mut layers = layers.to_vec();
        for layer in &mut layers {
            validate_policy(&mut layer.policy, Path::new("<layer>"))?;
        }
        merge(&layers).map(Self)
    }

    /// Read-only internal representation for the planning layers.
    #[doc(hidden)]
    pub fn as_merged(&self) -> &crate::layers::Policy {
        &self.0
    }

    pub fn mounts_mode(&self) -> ListMode {
        self.0.mounts_mode
    }
    pub fn network_mode(&self) -> NetworkMode {
        self.0.network_mode
    }
    pub fn environment_mode(&self) -> EnvMode {
        self.0.env_mode
    }
    pub fn commands_mode(&self) -> ListMode {
        self.0.commands_mode
    }
}

impl fmt::Debug for Policy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Policy")
            .field("mounts_mode", &self.mounts_mode())
            .field("network_mode", &self.network_mode())
            .field("environment_mode", &self.environment_mode())
            .field("commands_mode", &self.commands_mode())
            .finish_non_exhaustive()
    }
}
