//! The environment inside the isolation, assembled in the seven stages of specification
//! section 8, with the secrets of section 9 and the git rewrite of section 10. Pure: the
//! host environment and the secret file contents are given.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;

use crate::diagnostic::{Diagnostic, Warning};
use crate::layers::Policy;
use crate::policy::EnvMode;
use crate::wildcard::matches;

/// The most a secret's value may be: half of what Linux allows one environment variable
/// (128 KiB with the name and the terminator), so that the limit is reported as `secret`
/// rather than as a failed exec (specification section 9).
pub const SECRET_VALUE_LIMIT: usize = 64 * 1024;

/// What the outer layer found at a secret's file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretFile {
    Absent,
    Bytes(Vec<u8>),
    /// Exists but cannot be used, with the reason.
    Unreadable(String),
}

/// The final environment. The names of the secrets are kept so that any display masks
/// their values.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Environment {
    values: BTreeMap<OsString, OsString>,
    secret_names: BTreeSet<OsString>,
}

impl Environment {
    /// Every variable, secrets included: what the isolated process is started with.
    pub fn values(&self) -> &BTreeMap<OsString, OsString> {
        &self.values
    }

    /// The environment as it may be shown: a secret's value is `None` (specification
    /// section 9).
    pub fn shown(&self) -> BTreeMap<OsString, Option<OsString>> {
        self.values
            .iter()
            .map(|(name, value)| {
                let shown = (!self.secret_names.contains(name)).then(|| value.clone());
                (name.clone(), shown)
            })
            .collect()
    }
}

/// The environment and the warnings raised while assembling it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assembled {
    pub environment: Environment,
    pub warnings: Vec<Warning>,
}

type Values = BTreeMap<OsString, OsString>;

/// Assembles the environment of `policy` from `host`, the secret files, and the real
/// paths of the `path-prepend` entries, in the seven stages of specification section 8.
pub fn assemble_environment(
    policy: &Policy,
    host: &Values,
    secrets: &BTreeMap<String, SecretFile>,
    path_prepend: &[impl AsRef<Path>],
) -> Result<Assembled, Diagnostic> {
    let mut values = initial_values(policy, host);
    unset(&mut values, &policy.env_unset);
    // `set` is literal: no expansion in these values.
    for (name, value) in &policy.env_set {
        values.insert(OsString::from(name), OsString::from(value));
    }
    let (secret_names, warnings) = apply_secrets(&mut values, policy, secrets)?;
    apply_instead_of(&mut values, &policy.instead_of)?;
    prepend_path(&mut values, path_prepend);
    values.insert(OsString::from("PROCESS_WRAP"), OsString::from("1"));
    Ok(Assembled {
        environment: Environment {
            values,
            secret_names,
        },
        warnings,
    })
}

/// Stage 1: the host environment, or the `pass` variables alone.
fn initial_values(policy: &Policy, host: &Values) -> Values {
    match policy.env_mode {
        EnvMode::Inherit => host.clone(),
        EnvMode::Clear => policy
            .env_pass
            .iter()
            .filter_map(|name| {
                let name = OsStr::new(name);
                host.get(name)
                    .map(|value| (name.to_os_string(), value.clone()))
            })
            .collect(),
    }
}

/// Stage 2: `unset`, with wildcards.
fn unset(values: &mut Values, patterns: &[String]) {
    values.retain(|name, _| {
        !patterns
            .iter()
            .any(|pattern| matches(pattern, name.as_bytes()))
    });
}

/// Stage 4: each secret by name, removed whatever its origin and then set from its file
/// when there is one (specification section 9). Returns the names set and the warnings.
fn apply_secrets(
    values: &mut Values,
    policy: &Policy,
    secrets: &BTreeMap<String, SecretFile>,
) -> Result<(BTreeSet<OsString>, Vec<Warning>), Diagnostic> {
    let mut secret_names = BTreeSet::new();
    let mut warnings = Vec::new();
    for (name, path) in &policy.secrets {
        let variable = OsString::from(name);
        values.remove(&variable);
        match secrets.get(name).unwrap_or(&SecretFile::Absent) {
            SecretFile::Bytes(bytes) => {
                values.insert(
                    variable.clone(),
                    OsString::from_vec(secret_value(name, bytes)?),
                );
                secret_names.insert(variable);
            }
            SecretFile::Absent => warnings.push(Warning::new(format!(
                "the file of secret `{name}` ({path}) does not exist; the variable is not set"
            ))),
            SecretFile::Unreadable(reason) => {
                return Err(Diagnostic::secret(format!(
                    "the file of secret `{name}` ({path}) {reason}"
                )));
            }
        }
    }
    Ok((secret_names, warnings))
}

/// Stage 5: the git rewrite as `GIT_CONFIG_*` pairs numbered after the pairs already
/// there (specification section 10). Nothing is touched without entries.
fn apply_instead_of(
    values: &mut Values,
    instead_of: &BTreeMap<String, String>,
) -> Result<(), Diagnostic> {
    if instead_of.is_empty() {
        return Ok(());
    }
    let count = match values.get(OsStr::new("GIT_CONFIG_COUNT")) {
        None => 0,
        Some(count) => count
            .to_str()
            .filter(|text| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|text| text.parse::<usize>().ok())
            // The value may be a secret (specification section 9), so only the name is
            // reported.
            .ok_or_else(|| Diagnostic::env("GIT_CONFIG_COUNT is not a number"))?,
    };
    // A count the entries cannot be numbered after is as unusable to git as a
    // non-numeric one.
    let total = count
        .checked_add(instead_of.len())
        .ok_or_else(|| Diagnostic::env("GIT_CONFIG_COUNT is too large"))?;
    for (index, (original, replacement)) in instead_of.iter().enumerate() {
        let number = count + index;
        values.insert(
            OsString::from(format!("GIT_CONFIG_KEY_{number}")),
            OsString::from(format!("url.{replacement}.insteadof")),
        );
        values.insert(
            OsString::from(format!("GIT_CONFIG_VALUE_{number}")),
            OsString::from(original),
        );
    }
    values.insert(
        OsString::from("GIT_CONFIG_COUNT"),
        OsString::from(total.to_string()),
    );
    Ok(())
}

/// Stage 6: `path-prepend` in front of `PATH`; with no `PATH` it becomes `PATH`, and with
/// nothing to prepend an absent `PATH` stays absent.
fn prepend_path(values: &mut Values, path_prepend: &[impl AsRef<Path>]) {
    if path_prepend.is_empty() {
        return;
    }
    let mut path = OsString::new();
    for (index, entry) in path_prepend.iter().enumerate() {
        if index > 0 {
            path.push(":");
        }
        path.push(entry.as_ref());
    }
    if let Some(existing) = values.get(OsStr::new("PATH")) {
        path.push(":");
        path.push(existing);
    }
    values.insert(OsString::from("PATH"), path);
}

/// The value of a secret: the file's content without one trailing newline. The value
/// itself never enters the diagnostic.
fn secret_value(name: &str, bytes: &[u8]) -> Result<Vec<u8>, Diagnostic> {
    let value = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    if value.is_empty() {
        return Err(Diagnostic::secret(format!(
            "the file of secret `{name}` is empty"
        )));
    }
    if value.contains(&0) {
        return Err(Diagnostic::secret(format!(
            "the file of secret `{name}` contains a NUL byte"
        )));
    }
    if value.len() > SECRET_VALUE_LIMIT {
        return Err(Diagnostic::secret(format!(
            "the value of secret `{name}` is longer than {SECRET_VALUE_LIMIT} bytes"
        )));
    }
    Ok(value.to_vec())
}
