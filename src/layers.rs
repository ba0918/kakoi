//! The written layers of a policy and their merge (specification sections 5.3 and 5.5).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::cli::Invocation;
use crate::diagnostic::Diagnostic;
use crate::policy::{parse_policy, EnvMode, HideMounts, NetworkMode, PolicyFile, PolicyPath, Scan};
use crate::regular_file::{read_regular_file, Links};

/// Where a written layer came from, lowest first: the profile, the `--policy-file`, the
/// command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerOrigin {
    Profile(PathBuf),
    PolicyFile(PathBuf),
    CommandLine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    pub origin: LayerOrigin,
    pub policy: PolicyFile,
}

impl Layer {
    /// The command-line layer: `--rw` and `--hide`, already absolute.
    pub fn command_line(invocation: &Invocation) -> Self {
        let mut policy = PolicyFile::default();
        policy.mounts.rw = invocation
            .rw
            .iter()
            .cloned()
            .map(PolicyPath::Absolute)
            .collect();
        policy.mounts.hide = invocation
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

/// The four directives of the mount table (specification section 6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Directive {
    Rw,
    RwFile,
    Ro,
    Hide,
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
    pub mounts: Vec<MountItem>,
    pub scan: Vec<Scan>,
    pub hide_mounts: Vec<HideMounts>,
    pub network_mode: NetworkMode,
    pub env_mode: EnvMode,
    pub env_pass: Vec<String>,
    pub env_set: BTreeMap<String, String>,
    pub env_unset: Vec<String>,
    pub path_prepend: Vec<PolicyPath>,
    pub secrets: BTreeMap<String, PolicyPath>,
    pub instead_of: BTreeMap<String, String>,
}

/// Merges the written layers, lowest first: lists concatenate (the upper layer appended,
/// except `path-prepend` where it goes first), scalars take the upper layer, tables merge by
/// key with the upper layer winning.
pub fn merge(layers: &[Layer]) -> Result<Policy, Diagnostic> {
    let mut policy = Policy {
        mounts: Vec::new(),
        scan: Vec::new(),
        hide_mounts: Vec::new(),
        network_mode: NetworkMode::Host,
        env_mode: EnvMode::Inherit,
        env_pass: Vec::new(),
        env_set: BTreeMap::new(),
        env_unset: Vec::new(),
        path_prepend: Vec::new(),
        secrets: BTreeMap::new(),
        instead_of: BTreeMap::new(),
    };
    for layer in layers {
        let file = &layer.policy;
        for (directive, paths) in [
            (Directive::Rw, &file.mounts.rw),
            (Directive::RwFile, &file.mounts.rw_file),
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
    Ok(policy)
}

/// Reads the profile and the `--policy-file` named by `invocation` and returns the written
/// layers, lowest first. Reads nothing else.
pub fn load_layers(invocation: &Invocation, config_dir: &Path) -> Result<Vec<Layer>, Diagnostic> {
    let profile_path = config_dir
        .join("profile")
        .join(format!("{}.toml", invocation.profile));
    let mut layers = vec![Layer {
        origin: LayerOrigin::Profile(profile_path.clone()),
        policy: read_policy_file(&profile_path, || {
            format!("profile `{}`", invocation.profile)
        })?,
    }];
    if let Some(path) = &invocation.policy_file {
        layers.push(Layer {
            origin: LayerOrigin::PolicyFile(path.clone()),
            policy: read_policy_file(path, || "policy file".to_string())?,
        });
    }
    layers.push(Layer::command_line(invocation));
    Ok(layers)
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
