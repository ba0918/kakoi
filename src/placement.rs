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
    variables: &Variables,
    home: &HomeDirectory,
    current_dir: &Path,
    facts: &MountFacts,
) -> Result<Vec<Warning>, Diagnostic> {
    let writable: Vec<&ResolvedItem> = resolved
        .items
        .iter()
        .filter(|item| matches!(item.directive, Directive::Rw | Directive::RwFile))
        .collect();
    check_protected_paths(protected, &writable, facts)?;
    check_width(&writable, home)?;
    check_work_place(variables, home)?;
    check_current_dir(&resolved.items, current_dir)?;
    Ok(work_place_warning(&resolved.items, variables)
        .into_iter()
        .collect())
}

/// Specification section 5.6, in the order section 13 reports: the policy files, the
/// configuration directory, the secret files, the `path-prepend` entries.
fn check_protected_paths(
    protected: &ProtectedPaths,
    writable: &[&ResolvedItem],
    facts: &MountFacts,
) -> Result<(), Diagnostic> {
    for path in &protected.policy_files {
        check_prefixes(path, "the policy file", writable, facts)?;
    }
    check_prefixes(
        &protected.config_dir,
        "the configuration directory",
        writable,
        facts,
    )?;
    // A secret file inside `rw` could be swapped for a link to any host file, whose
    // content the next start would bring into the isolation as a variable.
    for (name, path) in &protected.secrets {
        check_prefixes(
            path,
            &format!("the file of secret `{name}`"),
            writable,
            facts,
        )?;
    }
    for path in &protected.path_prepend {
        check_prefixes(path, "the `path-prepend` entry", writable, facts)?;
    }
    Ok(())
}

/// Making the whole home writable is refused from every layer (specification
/// section 5.6).
fn check_width(writable: &[&ResolvedItem], home: &HomeDirectory) -> Result<(), Diagnostic> {
    match writable
        .iter()
        .find(|item| is_or_ancestor_of(&item.real, home.path()))
    {
        Some(item) => Err(Diagnostic::path(format!(
            "the `{}` item {} is `/`, the home directory, or an ancestor of it",
            directive_name(item.directive),
            item.real.display()
        ))),
        None => Ok(()),
    }
}

/// A worktree or workspace this wide would make the whole home the work place
/// (specification section 6.5); running from an unmanaged home without `--workspace`
/// lands here.
fn check_work_place(variables: &Variables, home: &HomeDirectory) -> Result<(), Diagnostic> {
    for (name, path) in [
        ("worktree", &variables.worktree),
        ("workspace", &variables.workspace),
    ] {
        if is_or_ancestor_of(path, home.path()) {
            return Err(Diagnostic::path(format!(
                "the {name} {} is `/`, the home directory, or an ancestor of it",
                path.display()
            )));
        }
    }
    Ok(())
}

/// The items are in the order they are applied, so the last one containing the current
/// directory is what bwrap would `--chdir` into; a `hide` there has nowhere to go
/// (specification section 6.5).
fn check_current_dir(items: &[ResolvedItem], current_dir: &Path) -> Result<(), Diagnostic> {
    let last_over_cwd = items
        .iter()
        .rfind(|item| current_dir.starts_with(&item.real));
    match last_over_cwd.filter(|item| item.directive == Directive::Hide) {
        Some(item) => Err(Diagnostic::path(format!(
            "the current directory {} is inside the `hide` item {}",
            current_dir.display(),
            item.real.display()
        ))),
        None => Ok(()),
    }
}

/// The warning of specification section 6.5 when no `rw` covers the work place.
fn work_place_warning(items: &[ResolvedItem], variables: &Variables) -> Option<Warning> {
    let covered = items.iter().any(|item| {
        item.directive == Directive::Rw
            && (variables.workspace.starts_with(&item.real)
                || variables.worktree.starts_with(&item.real))
    });
    (!covered).then(|| {
        Warning::new(format!(
            "no `rw` item covers the workspace {} or the worktree {}",
            variables.workspace.display(),
            variables.worktree.display()
        ))
    })
}

/// Every prefix of `path` (itself and each ancestor) that exists must not have its real
/// path inside a writable item, and no symbolic link the resolution passes through may
/// itself sit inside one: either could be re-pointed from inside the isolation. The
/// links are not all prefixes of `path`: a link's target may pass through further links.
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
    for link in facts.traversed_links(path) {
        if let Some(item) = writable.iter().find(|item| link.starts_with(&item.real)) {
            return Err(Diagnostic::path(format!(
                "the symbolic link {} on the way to {role} {} is inside the `{}` item {} and \
                 could be re-pointed from inside the isolation",
                link.display(),
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

/// Whether `candidate` is `/`, `path`, or an ancestor of `path`. `/` is named on its own so
/// that the rule holds whatever `path` looks like.
fn is_or_ancestor_of(candidate: &Path, path: &Path) -> bool {
    candidate == Path::new("/") || path.ancestors().any(|ancestor| ancestor == candidate)
}
