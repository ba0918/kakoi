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
    Host,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvMode {
    Inherit,
    Clear,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct PolicyFile {
    pub mounts: Mounts,
    pub network: Network,
    pub env: Env,
    pub secrets: BTreeMap<String, PolicyPath>,
    pub git: Git,
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
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Network {
    pub mode: Option<NetworkMode>,
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
    let policy: PolicyFile = toml::from_str(text).map_err(|error| {
        Diagnostic::policy(format!("{}: {}", origin.display(), error.message()))
    })?;
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
    Ok(policy)
}
