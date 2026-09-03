//! The variables of the host environment that `process-wrap` trusts (specification
//! section 14), the home directory checked from them (section 2), and the configuration
//! directory derived from both.

use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HostEnvironment {
    pub home: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
}

/// What the outer layer found behind a path: nothing (the path does not exist or cannot be
/// followed), a directory, or something else, each with its real path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealEntry {
    Missing,
    Directory(PathBuf),
    NotDirectory(PathBuf),
}

impl RealEntry {
    /// The real path, when something exists.
    pub fn path(&self) -> Option<&Path> {
        match self {
            RealEntry::Missing => None,
            RealEntry::Directory(path) | RealEntry::NotDirectory(path) => Some(path),
        }
    }
}

/// The home directory: the real path of `HOME`, after the checks of specification
/// section 2. `~` expands to it and the configuration directory falls back to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeDirectory(PathBuf);

impl HomeDirectory {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl HostEnvironment {
    /// Reads `HOME` and `XDG_CONFIG_HOME` from the process environment.
    pub fn from_process() -> Self {
        Self {
            home: std::env::var_os("HOME").map(PathBuf::from),
            xdg_config_home: std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        }
    }

    /// Checks `HOME` against `real`, what the outer layer found behind it: unset, empty, not
    /// absolute, without a real path, or not a directory is the `env` diagnostic.
    pub fn home_directory(&self, real: &RealEntry) -> Result<HomeDirectory, Diagnostic> {
        let home = self
            .home
            .as_deref()
            .ok_or_else(|| Diagnostic::env("HOME is not set"))?;
        if home.as_os_str().is_empty() {
            return Err(Diagnostic::env("HOME is empty"));
        }
        if !home.is_absolute() {
            return Err(Diagnostic::env(format!(
                "HOME is not an absolute path: {}",
                home.display()
            )));
        }
        match real {
            RealEntry::Directory(path) => Ok(HomeDirectory(path.clone())),
            RealEntry::NotDirectory(path) => Err(Diagnostic::env(format!(
                "HOME is not a directory: {}",
                path.display()
            ))),
            RealEntry::Missing => Err(Diagnostic::env(format!(
                "HOME has no real path: {}",
                home.display()
            ))),
        }
    }

    /// The configuration directory: `$XDG_CONFIG_HOME/process-wrap`, or
    /// `~/.config/process-wrap` when `XDG_CONFIG_HOME` is unset, empty, or not absolute.
    pub fn config_dir(&self, home: &HomeDirectory) -> PathBuf {
        let base = match &self.xdg_config_home {
            Some(xdg) if xdg.is_absolute() => xdg.clone(),
            _ => home.path().join(".config"),
        };
        base.join("process-wrap")
    }
}
