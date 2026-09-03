//! A synthetic host for the pure tests: a home at `/home/u`, a configuration directory
//! under it, a worktree at `/home/u/proj`, and facts declared per test.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use process_wrap::cli::Invocation;
use process_wrap::environment::{HomeDirectory, HostEnvironment, RealEntry};
use process_wrap::layers::{merge, Layer, LayerOrigin, Policy};
use process_wrap::policy::parse_policy;
use process_wrap::variables::Variables;

pub const HOME: &str = "/home/u";
pub const CONFIG_DIR: &str = "/home/u/.config/process-wrap";
pub const PROFILE: &str = "/home/u/.config/process-wrap/profile/default.toml";
pub const POLICY_FILE: &str = "/home/u/policies/p.toml";
pub const WORKTREE: &str = "/home/u/proj";

/// The checked home directory, whose real path is `/home/u`.
pub fn home() -> HomeDirectory {
    HostEnvironment {
        home: Some(PathBuf::from(HOME)),
        xdg_config_home: None,
    }
    .home_directory(&RealEntry::Directory(PathBuf::from(HOME)))
    .unwrap()
}

/// The variables of a workspace at the worktree with a `.git` directory.
pub fn variables() -> Variables {
    Variables {
        workspace: PathBuf::from(WORKTREE),
        worktree: PathBuf::from(WORKTREE),
        git_common_dir: Some(PathBuf::from(WORKTREE).join(".git")),
        config_dir: PathBuf::from(CONFIG_DIR),
    }
}

pub fn profile_layer(text: &str) -> Layer {
    Layer {
        origin: LayerOrigin::Profile(PathBuf::from(PROFILE)),
        policy: parse_policy(text, Path::new(PROFILE)).unwrap(),
    }
}

pub fn policy_file_layer(text: &str) -> Layer {
    Layer {
        origin: LayerOrigin::PolicyFile(PathBuf::from(POLICY_FILE)),
        policy: parse_policy(text, Path::new(POLICY_FILE)).unwrap(),
    }
}

pub fn command_line_layer(rw: &[&str], hide: &[&str]) -> Layer {
    Layer::command_line(&Invocation {
        profile: "default".to_string(),
        policy_file: None,
        workspace: None,
        rw: rw.iter().map(PathBuf::from).collect(),
        hide: hide.iter().map(PathBuf::from).collect(),
        print_plan: false,
        command: vec![OsString::from("true")],
    })
}

/// The three written layers: the profile text, an optional policy file text, and the
/// command line's `--rw` and `--hide`.
pub fn layers(profile: &str, policy_file: Option<&str>, rw: &[&str], hide: &[&str]) -> Vec<Layer> {
    let mut layers = vec![profile_layer(profile)];
    if let Some(text) = policy_file {
        layers.push(policy_file_layer(text));
    }
    layers.push(command_line_layer(rw, hide));
    layers
}

pub fn merged(layers: &[Layer]) -> Policy {
    merge(layers).unwrap()
}

/// Facts about paths: what exists behind each. A path not listed is missing.
#[derive(Debug, Default, Clone)]
pub struct Facts(pub BTreeMap<PathBuf, RealEntry>);

impl Facts {
    pub fn new() -> Self {
        Self::default()
    }

    /// A directory whose real path is itself.
    pub fn dir(mut self, path: &str) -> Self {
        self.0.insert(
            PathBuf::from(path),
            RealEntry::Directory(PathBuf::from(path)),
        );
        self
    }

    /// A non-directory (a regular file, a socket, a FIFO) whose real path is itself.
    pub fn file(mut self, path: &str) -> Self {
        self.0.insert(
            PathBuf::from(path),
            RealEntry::NotDirectory(PathBuf::from(path)),
        );
        self
    }

    /// A symbolic link at `path` to the directory `real`.
    pub fn link_to_dir(mut self, path: &str, real: &str) -> Self {
        self.0.insert(
            PathBuf::from(path),
            RealEntry::Directory(PathBuf::from(real)),
        );
        self
    }

    /// A symbolic link at `path` to the non-directory `real`.
    pub fn link_to_file(mut self, path: &str, real: &str) -> Self {
        self.0.insert(
            PathBuf::from(path),
            RealEntry::NotDirectory(PathBuf::from(real)),
        );
        self
    }

    /// Every ancestor of `path` as a directory whose real path is itself, and `path` as a
    /// regular file.
    pub fn file_with_ancestors(self, path: &str) -> Self {
        self.ancestors_of(path).file(path)
    }

    /// Every ancestor of `path` as a directory whose real path is itself, and `path` too.
    pub fn dir_with_ancestors(self, path: &str) -> Self {
        self.ancestors_of(path).dir(path)
    }

    fn ancestors_of(mut self, path: &str) -> Self {
        for ancestor in Path::new(path).ancestors().skip(1) {
            let text = ancestor.to_str().unwrap();
            self = self.dir(text);
        }
        self
    }
}

/// The variables of a workspace at the worktree that is not under git.
pub fn variables_without_git() -> Variables {
    Variables {
        git_common_dir: None,
        ..variables()
    }
}

/// A configuration directory outside the home, as `XDG_CONFIG_HOME=/etc/xdg` gives.
pub const XDG_CONFIG_DIR: &str = "/etc/xdg/process-wrap";
pub const XDG_PROFILE: &str = "/etc/xdg/process-wrap/profile/default.toml";

pub fn xdg_profile_layer(text: &str) -> Layer {
    Layer {
        origin: LayerOrigin::Profile(PathBuf::from(XDG_PROFILE)),
        policy: parse_policy(text, Path::new(XDG_PROFILE)).unwrap(),
    }
}

/// The variables with the configuration directory outside the home.
pub fn xdg_variables() -> Variables {
    Variables {
        config_dir: PathBuf::from(XDG_CONFIG_DIR),
        ..variables()
    }
}
