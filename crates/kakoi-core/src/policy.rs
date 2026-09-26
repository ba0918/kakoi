//! The policy file: its TOML schema and the format rules of specification sections 5.1
//! and 5.2.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::diagnostic::Diagnostic;

const VARIABLES: [(&str, Variable); 4] = [
    ("workspace", Variable::Workspace),
    ("worktree", Variable::Worktree),
    ("git_common_dir", Variable::GitCommonDir),
    ("config_dir", Variable::ConfigDir),
];

/// One of the four variables a policy path may start with (specification section 5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variable {
    Workspace,
    Worktree,
    GitCommonDir,
    ConfigDir,
}

/// A path as written in a policy file: absolute, under the home directory, or starting with
/// a variable. The `String` is the rest of the text after `~` or `${...}`, kept literally.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub enum PolicyPath {
    Absolute(PathBuf),
    Home(String),
    Variable(Variable, String),
}

impl TryFrom<String> for PolicyPath {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        if text.starts_with('/') {
            return Ok(PolicyPath::Absolute(PathBuf::from(text)));
        }
        if let Some(rest) = text.strip_prefix('~') {
            if rest.is_empty() || rest.starts_with('/') {
                return Ok(PolicyPath::Home(rest.to_string()));
            }
            return Err(format!("{text:?}: the `~user` form is not accepted"));
        }
        if let Some(rest) = text.strip_prefix("${") {
            let Some((name, rest)) = rest.split_once('}') else {
                return Err(format!("{text:?}: unterminated variable"));
            };
            return match VARIABLES.iter().find(|(known, _)| *known == name) {
                Some((_, variable)) => Ok(PolicyPath::Variable(*variable, rest.to_string())),
                None => Err(format!("{text:?}: unknown variable `{name}`")),
            };
        }
        Err(format!(
            "{text:?}: a path must be absolute, start with `~`, or start with a variable"
        ))
    }
}

impl Variable {
    pub fn name(self) -> &'static str {
        VARIABLES
            .iter()
            .find(|(_, variable)| *variable == self)
            .map(|(name, _)| *name)
            .expect("every variable is in the table")
    }
}

/// The path as it was written.
impl fmt::Display for PolicyPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PolicyPath::Absolute(path) => write!(f, "{}", path.display()),
            PolicyPath::Home(rest) => write!(f, "~{rest}"),
            PolicyPath::Variable(variable, rest) => write!(f, "${{{}}}{rest}", variable.name()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    Filtered,
    Host,
    None,
}

impl NetworkMode {
    /// The word a policy writes the mode with.
    pub fn name(self) -> &'static str {
        match self {
            NetworkMode::Filtered => "filtered",
            NetworkMode::Host => "host",
            NetworkMode::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvMode {
    Inherit,
    Clear,
}

impl EnvMode {
    /// The word a policy writes the mode with.
    pub fn name(self) -> &'static str {
        match self {
            EnvMode::Inherit => "inherit",
            EnvMode::Clear => "clear",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct PolicyFile {
    pub mounts: Mounts,
    pub network: Network,
    pub process: Process,
    pub env: Env,
    pub secrets: BTreeMap<String, PolicyPath>,
    pub git: Git,
    pub commands: crate::guard::Commands,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Process {
    pub shutdown_grace_seconds: Option<u32>,
}

impl Process {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .shutdown_grace_seconds
            .is_some_and(|value| !(1..=300).contains(&value))
        {
            return Err("process.shutdown-grace-seconds must be in 1..=300".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Mounts {
    pub rw: Vec<PolicyPath>,
    pub rw_file: Vec<PolicyPath>,
    pub rw_copy: Vec<PolicyPath>,
    pub ro: Vec<PolicyPath>,
    pub hide: Vec<PolicyPath>,
    pub scan: Vec<Scan>,
    pub hide_mounts: Vec<HideMounts>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Scan {
    pub root: PolicyPath,
    pub names: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub prune: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct HideMounts {
    pub under: PolicyPath,
    pub fstype: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(from = "NetworkInput")]
pub struct Network {
    pub mode: Option<NetworkMode>,
    pub publish: Vec<crate::network::FixedPublication>,
    pub allow: Vec<crate::network::Allow>,
    pub limits: crate::network::LimitOverrides,
    pub dns_upstream: Vec<crate::network::DnsUpstream>,
    pub(crate) settings_present: bool,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
struct NetworkInput {
    mode: Option<NetworkMode>,
    publish: Option<Vec<crate::network::FixedPublication>>,
    allow: Option<Vec<crate::network::Allow>>,
    dns_upstream: Option<Vec<crate::network::DnsUpstream>>,
    #[serde(flatten)]
    limits: crate::network::LimitOverrides,
}

impl From<NetworkInput> for Network {
    fn from(input: NetworkInput) -> Self {
        Self {
            mode: input.mode,
            settings_present: input.publish.is_some()
                || input.dns_upstream.is_some()
                || input.allow.is_some()
                || input.limits.is_present(),
            publish: input.publish.unwrap_or_default(),
            allow: input.allow.unwrap_or_default(),
            limits: input.limits,
            dns_upstream: input.dns_upstream.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Env {
    pub mode: Option<EnvMode>,
    pub pass: Vec<String>,
    pub set: BTreeMap<String, String>,
    pub unset: Vec<String>,
    pub path_prepend: Vec<PolicyPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Git {
    pub instead_of: BTreeMap<String, String>,
}

/// Parses the text of one policy file. `origin` names the file in diagnostics.
pub fn parse_policy(text: &str, origin: &Path) -> Result<PolicyFile, Diagnostic> {
    let mut policy: PolicyFile = toml::from_str(text).map_err(|error| {
        Diagnostic::policy(format!("{}: {}", origin.display(), error.message()))
    })?;
    policy
        .process
        .validate()
        .map_err(|error| Diagnostic::policy(format!("{}: {error}", origin.display())))?;
    for scan in &policy.mounts.scan {
        if scan.names.is_empty() {
            return Err(Diagnostic::policy(format!(
                "{}: `mounts.scan` needs a non-empty `names`",
                origin.display()
            )));
        }
    }
    for hide_mounts in &policy.mounts.hide_mounts {
        if hide_mounts.fstype.is_empty() {
            return Err(Diagnostic::policy(format!(
                "{}: `mounts.hide-mounts` needs a non-empty `fstype`",
                origin.display()
            )));
        }
    }
    for rule in &policy.commands.guard {
        rule.validate()
            .map_err(|error| Diagnostic::policy(format!("{}: {error}", origin.display())))?;
    }
    policy.network.publish = crate::network::merge_publications(policy.network.publish)
        .map_err(|error| Diagnostic::policy(format!("{}: {error}", origin.display())))?;
    policy
        .network
        .limits
        .validate()
        .map_err(|error| Diagnostic::policy(format!("{}: {error}", origin.display())))?;
    crate::network::validate_upstreams(&policy.network.dns_upstream)
        .map_err(|error| Diagnostic::policy(format!("{}: {error}", origin.display())))?;
    Ok(policy)
}
