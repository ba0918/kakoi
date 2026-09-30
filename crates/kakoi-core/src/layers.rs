//! The written layers of a policy and their merge (specification sections 5.3 and 5.5).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::environment::PathState;
use crate::guard::GuardRule;
use crate::policy::{
    parse_policy, EnvMode, HideMounts, ListMode, NetworkMode, PolicyFile, PolicyPath, Scan,
};
use crate::regular_file::{read_regular_file, Links};
use crate::workspace_facts::probe_path;

/// The bundled profile compiled into the binary: the built-in default (specification
/// section 2), what `kakoi init` writes out.
pub const BUILT_IN_DEFAULT: &str = include_str!("../../../examples/profile/default.toml");

/// The example that shows only what it lists, which `kakoi init NAME --example listed`
/// writes out (specification REQ-481).
pub const LISTED_EXAMPLE: &str = include_str!("../../../examples/profile/listed.toml");

/// The profile of the global scope when `--profile` is omitted (specification
/// section 4.1). Only this name falls back to the built-in default (section 5.3).
pub const DEFAULT_PROFILE: &str = "default";

/// What the written layers are read from, as the command line selects them: the profile
/// name, the `--policy-file` path, and the `--rw` and `--hide` paths of the command-line
/// layer. The paths are absolute already; nothing here is interpreted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerSelection {
    pub profile: String,
    pub policy_file: Option<PathBuf>,
    pub rw: Vec<PathBuf>,
    pub hide: Vec<PathBuf>,
}

/// Where a written layer came from, lowest first: the profile (a file or the built-in
/// default), the `--policy-file`, the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerOrigin {
    Profile(PathBuf),
    BuiltInDefault,
    PolicyFile(PathBuf),
    CommandLine,
}

/// Where a policy the run read came from, as the plan names it: a file at its real path,
/// or the built-in default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicySource {
    File(PathBuf),
    BuiltInDefault,
}

impl PolicySource {
    pub fn path(&self) -> Option<&Path> {
        match self {
            PolicySource::File(path) => Some(path),
            PolicySource::BuiltInDefault => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    pub origin: LayerOrigin,
    pub policy: PolicyFile,
}

impl Layer {
    /// The command-line layer: `--rw` and `--hide`, already absolute.
    pub fn command_line(selection: &LayerSelection) -> Self {
        let mut policy = PolicyFile::default();
        policy.mounts.rw = selection
            .rw
            .iter()
            .cloned()
            .map(PolicyPath::Absolute)
            .collect();
        policy.mounts.hide = selection
            .hide
            .iter()
            .cloned()
            .map(PolicyPath::Absolute)
            .collect();
        Self {
            origin: LayerOrigin::CommandLine,
            policy,
        }
    }
}

/// The five directives of the mount table (specification section 6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Directive {
    Rw,
    RwFile,
    /// The host's content to start from, writable inside, and nothing written reaching
    /// the host: a tmpfs seeded with what is at the real path, or, for a regular file, a
    /// bound copy of its bytes.
    RwCopy,
    Ro,
    Hide,
}

impl Directive {
    /// The word a policy writes the directive with.
    pub fn name(self) -> &'static str {
        match self {
            Directive::Rw => "rw",
            Directive::RwFile => "rw-file",
            Directive::RwCopy => "rw-copy",
            Directive::Ro => "ro",
            Directive::Hide => "hide",
        }
    }
}

/// One written mount item, with the layer it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountItem {
    pub directive: Directive,
    pub path: PolicyPath,
    pub origin: LayerOrigin,
}

/// The merged policy. Mount items keep their layer and appear lower layer first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// `listed` once any layer writes it (specification TBL-152).
    pub mounts_mode: ListMode,
    /// Whether the base directories are shown; off once any layer turns it off.
    pub mounts_system: bool,
    pub mounts: Vec<MountItem>,
    pub scan: Vec<Scan>,
    pub hide_mounts: Vec<HideMounts>,
    pub network_mode: NetworkMode,
    pub network_settings_present: bool,
    pub network_publish: Vec<crate::network::FixedPublication>,
    pub network_allow: Vec<crate::network::Allow>,
    pub network_limits: crate::network::NetworkLimits,
    pub dns_upstream: Vec<crate::network::DnsUpstream>,
    pub shutdown_grace_seconds: u32,
    /// Whether the host's `/dev/net/tun` is shown inside (specification REQ-458).
    pub allow_nested_filtered: bool,
    pub env_mode: EnvMode,
    pub env_pass: Vec<String>,
    pub env_set: BTreeMap<String, String>,
    pub env_unset: Vec<String>,
    pub path_prepend: Vec<PolicyPath>,
    pub secrets: BTreeMap<String, PolicyPath>,
    pub instead_of: BTreeMap<String, String>,
    /// `listed` once any layer writes it (specification TBL-152).
    pub commands_mode: ListMode,
    /// The programs a `listed` command mode lets start, lower layer first.
    pub commands_allow: Vec<PolicyPath>,
    /// The rules of `commands.guard`, lower layer first, each with its layer.
    pub guards: Vec<GuardEntry>,
}

/// One rule of `commands.guard` with the layer it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardEntry {
    pub rule: GuardRule,
    pub origin: LayerOrigin,
}

/// Merges the written layers, lowest first: lists concatenate (the upper layer appended,
/// except `path-prepend` where it goes first), scalars take the upper layer, tables merge by
/// key with the upper layer winning.
pub fn merge(layers: &[Layer]) -> Result<Policy, Diagnostic> {
    let network_settings_present = layers.iter().any(|layer| {
        let network = &layer.policy.network;
        layer.policy.process.shutdown_grace_seconds.is_some()
            || network.settings_present
            || !network.allow.is_empty()
            || !network.dns_upstream.is_empty()
            || !network.publish.is_empty()
            || network.limits.is_present()
    });
    if network_settings_present
        && !layers
            .iter()
            .any(|layer| layer.policy.network.mode.is_some())
    {
        return Err(Diagnostic::policy(
            "network settings require an explicit `network.mode` in a layer",
        ));
    }
    let mut policy = Policy {
        mounts_mode: strictest_mode(layers.iter().map(|layer| layer.policy.mounts.mode)),
        mounts_system: layers
            .iter()
            .all(|layer| layer.policy.mounts.system != Some(false)),
        mounts: Vec::new(),
        scan: Vec::new(),
        hide_mounts: Vec::new(),
        network_mode: NetworkMode::Host,
        network_settings_present,
        network_publish: Vec::new(),
        network_allow: Vec::new(),
        network_limits: crate::network::NetworkLimits::default(),
        dns_upstream: Vec::new(),
        shutdown_grace_seconds: 5,
        allow_nested_filtered: false,
        env_mode: EnvMode::Inherit,
        env_pass: Vec::new(),
        env_set: BTreeMap::new(),
        env_unset: Vec::new(),
        path_prepend: Vec::new(),
        secrets: BTreeMap::new(),
        instead_of: BTreeMap::new(),
        commands_mode: strictest_mode(layers.iter().map(|layer| layer.policy.commands.mode)),
        commands_allow: Vec::new(),
        guards: Vec::new(),
    };
    for layer in layers {
        let file = &layer.policy;
        file.process.validate().map_err(Diagnostic::policy)?;
        if let Some(seconds) = file.process.shutdown_grace_seconds {
            policy.shutdown_grace_seconds = seconds;
        }
        for (directive, paths) in [
            (Directive::Rw, &file.mounts.rw),
            (Directive::RwFile, &file.mounts.rw_file),
            (Directive::RwCopy, &file.mounts.rw_copy),
            (Directive::Ro, &file.mounts.ro),
            (Directive::Hide, &file.mounts.hide),
        ] {
            policy.mounts.extend(paths.iter().map(|path| MountItem {
                directive,
                path: path.clone(),
                origin: layer.origin.clone(),
            }));
        }
        policy.scan.extend(file.mounts.scan.iter().cloned());
        policy
            .hide_mounts
            .extend(file.mounts.hide_mounts.iter().cloned());
        if let Some(mode) = file.network.mode {
            policy.network_mode = mode;
        }
        if let Some(allow) = file.network.allow_nested_filtered {
            policy.allow_nested_filtered = allow;
        }
        policy.network_publish.extend(&file.network.publish);
        policy
            .dns_upstream
            .extend(file.network.dns_upstream.iter().cloned());
        policy.network_limits = file
            .network
            .limits
            .apply(&policy.network_limits)
            .map_err(Diagnostic::policy)?;
        for rule in &file.network.allow {
            if !policy.network_allow.contains(rule) {
                policy.network_allow.push(rule.clone());
            }
        }
        if let Some(mode) = file.env.mode {
            policy.env_mode = mode;
        }
        policy.env_pass.extend(file.env.pass.iter().cloned());
        policy.env_set.extend(file.env.set.clone());
        policy.env_unset.extend(file.env.unset.iter().cloned());
        policy
            .path_prepend
            .splice(0..0, file.env.path_prepend.iter().cloned());
        policy.secrets.extend(file.secrets.clone());
        policy.instead_of.extend(file.git.instead_of.clone());
        policy
            .commands_allow
            .extend(file.commands.allow.iter().cloned());
        policy
            .guards
            .extend(file.commands.guard.iter().map(|rule| GuardEntry {
                rule: rule.clone(),
                origin: layer.origin.clone(),
            }));
    }
    if policy.mounts_mode != ListMode::Listed && !policy.mounts_system {
        return Err(Diagnostic::policy(
            "`mounts.system = false` has no effect while the merged `mounts.mode` is not `listed`",
        ));
    }
    if policy.commands_mode != ListMode::Listed && !policy.commands_allow.is_empty() {
        return Err(Diagnostic::policy(
            "`commands.allow` has no effect while the merged `commands.mode` is not `listed`",
        ));
    }
    if policy.env_mode == EnvMode::Inherit && !policy.env_pass.is_empty() {
        return Err(Diagnostic::policy(
            "`env.pass` has no effect while the merged `env.mode` is `inherit`",
        ));
    }
    if let Some(key) = policy
        .env_set
        .keys()
        .find(|key| policy.secrets.contains_key(*key))
    {
        return Err(Diagnostic::policy(format!(
            "`{key}` is in both `env.set` and `secrets` after merging"
        )));
    }
    if policy.network_mode == NetworkMode::Filtered
        && policy.network_allow.iter().any(|allow| {
            matches!(
                allow.destination,
                crate::network::Destination::Address {
                    host_interface: Some(_),
                    ..
                }
            )
        })
    {
        return Err(Diagnostic::policy(
            "filtered network mode does not support `host-interface` destinations",
        ));
    }
    policy.network_publish =
        crate::network::merge_publications(policy.network_publish).map_err(Diagnostic::policy)?;
    policy
        .network_limits
        .validate_deadlines()
        .map_err(Diagnostic::policy)?;
    crate::network::validate_upstreams(&policy.dns_upstream).map_err(Diagnostic::policy)?;
    Ok(policy)
}

/// The merged value of a mode no upper layer can loosen: `listed` once any layer writes
/// it, else `host` (specification TBL-152).
fn strictest_mode(mut written: impl Iterator<Item = Option<ListMode>>) -> ListMode {
    if written.any(|mode| mode == Some(ListMode::Listed)) {
        ListMode::Listed
    } else {
        ListMode::Host
    }
}

/// Reads the profile and the `--policy-file` named by `selection` and returns the written
/// layers, lowest first. Reads nothing else.
pub fn load_layers(
    selection: &LayerSelection,
    config_dir: &Path,
) -> Result<Vec<Layer>, Diagnostic> {
    let profile_path = config_dir
        .join("profile")
        .join(format!("{}.toml", selection.profile));
    let mut layers = vec![global_scope(selection, &profile_path)?];
    if let Some(path) = &selection.policy_file {
        layers.push(Layer {
            origin: LayerOrigin::PolicyFile(path.clone()),
            policy: read_policy_file(path, || "policy file".to_string())?,
        });
    }
    layers.push(Layer::command_line(selection));
    Ok(layers)
}

/// The global scope: the profile file, or the built-in default when `default.toml` has not
/// been written out (specification section 5.3). "Not written out" is the components of the
/// assembled path looked at from the root, the first failure being a name that does not
/// exist; a broken link or a regular file in the way is read and stops the run, so a policy
/// that broke is never replaced by the wider built-in default.
fn global_scope(selection: &LayerSelection, path: &Path) -> Result<Layer, Diagnostic> {
    if selection.profile == DEFAULT_PROFILE && probe_path(path) == PathState::Absent {
        return Ok(Layer {
            origin: LayerOrigin::BuiltInDefault,
            policy: parse_policy(BUILT_IN_DEFAULT, path)?,
        });
    }
    Ok(Layer {
        origin: LayerOrigin::Profile(path.to_path_buf()),
        policy: read_policy_file(path, || format!("profile `{}`", selection.profile))?,
    })
}

/// Reads one policy file following a symbolic link at its path (a user keeps policy files
/// in dotfiles behind links), within the reading rules of specification section 14.
fn read_policy_file(path: &Path, role: impl FnOnce() -> String) -> Result<PolicyFile, Diagnostic> {
    let text = read_regular_file(path, Links::Follow)
        .map_err(|error| error.to_string())
        .and_then(|bytes| String::from_utf8(bytes).map_err(|_| "is not valid UTF-8".to_string()))
        .map_err(|reason| {
            Diagnostic::policy(format!("{} at {} {reason}", role(), path.display()))
        })?;
    parse_policy(&text, path)
}
