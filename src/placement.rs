//! The checks on where things are placed, made on the resolved mount items: the places a
//! policy could be rewritten from inside the isolation (specification section 5.6) and the
//! rules about the work place and the current directory (section 6.5). Pure.

use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, Warning};
use crate::environment::HomeDirectory;
use crate::layers::{Directive, Layer, LayerOrigin};
use crate::mounts::{ExpandedPolicy, MountFacts, ResolvedItem, ResolvedMounts};
use crate::variables::Variables;

/// The paths specification section 5.6 protects, in the order section 13 reports them:
/// the policy files read (profile first), the configuration directory, the secret files
/// by name, and the `path-prepend` entries in merged order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProtectedPaths {
    pub policy_files: Vec<PathBuf>,
    pub config_dir: PathBuf,
    pub secrets: Vec<(String, PathBuf)>,
    pub path_prepend: Vec<PathBuf>,
}

pub fn protected_paths(
    expanded: &ExpandedPolicy,
    layers: &[Layer],
    config_dir: &Path,
) -> ProtectedPaths {
    ProtectedPaths {
        policy_files: layers
            .iter()
            .filter_map(|layer| match &layer.origin {
                LayerOrigin::Profile(path) | LayerOrigin::PolicyFile(path) => Some(path.clone()),
                LayerOrigin::CommandLine => None,
            })
            .collect(),
        config_dir: config_dir.to_path_buf(),
        secrets: expanded
            .secrets
            .iter()
            .filter_map(|(name, path)| Some((name.clone(), path.path()?.to_path_buf())))
            .collect(),
        path_prepend: expanded
            .path_prepend
            .iter()
            .filter_map(|path| Some(path.path()?.to_path_buf()))
            .collect(),
    }
}

/// Runs the checks in the order of specification section 13 and returns the warnings of
/// a run that may go on.
pub fn check_placement(
    resolved: &ResolvedMounts,
    protected: &ProtectedPaths,
    _variables: &Variables,
    _home: &HomeDirectory,
    _current_dir: &Path,
    facts: &MountFacts,
) -> Result<Vec<Warning>, Diagnostic> {
    let writable: Vec<&ResolvedItem> = resolved
        .items
        .iter()
        .filter(|item| matches!(item.directive, Directive::Rw | Directive::RwFile))
        .collect();
    for path in &protected.policy_files {
        check_prefixes(path, "the policy file", &writable, facts)?;
    }
    check_prefixes(
        &protected.config_dir,
        "the configuration directory",
        &writable,
        facts,
    )?;
    // A secret file inside `rw` could be swapped for a link to any host file, whose
    // content the next start would bring into the isolation as a variable.
    for (name, path) in &protected.secrets {
        check_prefixes(
            path,
            &format!("the file of secret `{name}`"),
            &writable,
            facts,
        )?;
    }
    for path in &protected.path_prepend {
        check_prefixes(path, "the `path-prepend` entry", &writable, facts)?;
    }
    Ok(Vec::new())
}

/// Every prefix of `path` (itself and each ancestor) that exists must not have its real
/// path inside a writable item: a link on the way could be re-pointed from inside the
/// isolation.
fn check_prefixes(
    path: &Path,
    role: &str,
    writable: &[&ResolvedItem],
    facts: &MountFacts,
) -> Result<(), Diagnostic> {
    for prefix in path.ancestors() {
        let Some(real) = facts.entry(prefix).path().map(Path::to_path_buf) else {
            continue;
        };
        if let Some(item) = writable.iter().find(|item| real.starts_with(&item.real)) {
            return Err(Diagnostic::path(format!(
                "{role} {} is inside the `{}` item {} and could be replaced from inside the \
                 isolation",
                path.display(),
                directive_name(item.directive),
                item.real.display()
            )));
        }
    }
    Ok(())
}

fn directive_name(directive: Directive) -> &'static str {
    match directive {
        Directive::Rw => "rw",
        Directive::RwFile => "rw-file",
        Directive::Ro => "ro",
        Directive::Hide => "hide",
    }
}
