//! The variables of the host environment that `process-wrap` trusts (specification
//! section 14), and the configuration directory derived from them.

use std::path::PathBuf;

use crate::diagnostic::Diagnostic;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostEnvironment {
    pub home: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
}

impl HostEnvironment {
    /// Reads `HOME` and `XDG_CONFIG_HOME` from the process environment.
    pub fn from_process() -> Self {
        Self {
            home: std::env::var_os("HOME").map(PathBuf::from),
            xdg_config_home: std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        }
    }

    /// The configuration directory: `$XDG_CONFIG_HOME/process-wrap`, or
    /// `~/.config/process-wrap` when `XDG_CONFIG_HOME` is unset.
    pub fn config_dir(&self) -> Result<PathBuf, Diagnostic> {
        let base = match &self.xdg_config_home {
            Some(xdg) => xdg.clone(),
            None => self.home()?.join(".config"),
        };
        Ok(base.join("process-wrap"))
    }

    /// `HOME`, or the `env` diagnostic when it is unset.
    pub fn home(&self) -> Result<&PathBuf, Diagnostic> {
        self.home
            .as_ref()
            .ok_or_else(|| Diagnostic::env("HOME is not set"))
    }
}
