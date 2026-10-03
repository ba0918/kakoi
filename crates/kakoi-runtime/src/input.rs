//! Request-local host state, without changes to process-global settings.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputError {
    pub field: &'static str,
    pub reason: &'static str,
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.reason)
    }
}

impl std::error::Error for InputError {}

#[derive(Clone, PartialEq, Eq)]
pub struct HostContext {
    cwd: PathBuf,
    env: BTreeMap<OsString, OsString>,
}

impl HostContext {
    pub fn new(cwd: PathBuf, env: BTreeMap<OsString, OsString>) -> Result<Self, InputError> {
        if !cwd.is_absolute() {
            return Err(InputError {
                field: "cwd",
                reason: "an absolute path is required",
            });
        }
        check_nul(cwd.as_os_str(), "cwd")?;
        for (name, value) in &env {
            check_nul(name, "environment name")?;
            if name.is_empty() || name.as_bytes().contains(&b'=') {
                return Err(InputError {
                    field: "environment name",
                    reason: "a nonempty name without '=' is required",
                });
            }
            check_nul(value, "environment value")?;
        }
        Ok(Self { cwd, env })
    }

    pub fn capture() -> Result<Self, std::io::Error> {
        Self::new(std::env::current_dir()?, std::env::vars_os().collect())
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidInput, error))
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn environment(&self) -> &BTreeMap<OsString, OsString> {
        &self.env
    }
}

impl fmt::Debug for HostContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostContext")
            .field("cwd", &self.cwd)
            .field("environment_names", &self.env.keys().collect::<Vec<_>>())
            .finish()
    }
}

fn check_nul(value: &OsStr, field: &'static str) -> Result<(), InputError> {
    if value.as_bytes().contains(&0) {
        Err(InputError {
            field,
            reason: "NUL is not permitted",
        })
    } else {
        Ok(())
    }
}
