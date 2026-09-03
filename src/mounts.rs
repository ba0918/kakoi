//! The mount items of the merged policy: the expansion of `~` and the variables
//! (specification section 5.2). Pure.

use std::path::PathBuf;

use crate::environment::HomeDirectory;
use crate::layers::{Directive, LayerOrigin, Policy};
use crate::policy::{PolicyPath, Variable};
use crate::variables::Variables;

/// A policy path after expansion: a path, or a variable that has no value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expansion {
    Path(PathBuf),
    Valueless(Variable),
}

/// One written mount item with its path expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedItem {
    pub directive: Directive,
    pub written: PolicyPath,
    pub origin: LayerOrigin,
    pub path: Expansion,
}

/// The merged policy with every path-taking value expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedPolicy {
    pub mounts: Vec<ExpandedItem>,
}

/// Expands `~` to the home directory and the variables to their values.
pub fn expand(path: &PolicyPath, variables: &Variables, home: &HomeDirectory) -> Expansion {
    let (base, rest) = match path {
        PolicyPath::Absolute(path) => return Expansion::Path(path.clone()),
        PolicyPath::Home(rest) => (home.path(), rest),
        PolicyPath::Variable(variable, rest) => {
            let value = match variable {
                Variable::Workspace => Some(variables.workspace.as_path()),
                Variable::Worktree => Some(variables.worktree.as_path()),
                Variable::GitCommonDir => variables.git_common_dir.as_deref(),
                Variable::ConfigDir => Some(variables.config_dir.as_path()),
            };
            match value {
                Some(value) => (value, rest),
                None => return Expansion::Valueless(*variable),
            }
        }
    };
    let mut expanded = base.as_os_str().to_os_string();
    expanded.push(rest);
    Expansion::Path(PathBuf::from(expanded))
}

/// Expands every path-taking value of `policy`.
pub fn expand_policy(
    policy: &Policy,
    variables: &Variables,
    home: &HomeDirectory,
) -> ExpandedPolicy {
    ExpandedPolicy {
        mounts: policy
            .mounts
            .iter()
            .map(|item| ExpandedItem {
                directive: item.directive,
                written: item.path.clone(),
                origin: item.origin.clone(),
                path: expand(&item.path, variables, home),
            })
            .collect(),
    }
}
