//! The written layers of a policy and their merge (specification sections 5.3 and 5.5).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;
use crate::environment::PathState;
use crate::policy::{parse_policy, EnvMode, HideMounts, NetworkMode, PolicyFile, PolicyPath, Scan};
use crate::regular_file::{read_regular_file, Links};
use crate::workspace_facts::probe_path;

/// The bundled profile compiled into the binary: the built-in default (specification
/// section 2), what `kakoi init` writes out.
pub const BUILT_IN_DEFAULT: &str = include_str!("../examples/profile/default.toml");

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
