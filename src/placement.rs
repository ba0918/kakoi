//! The checks on where things are placed, made on the resolved mount items: the places a
//! policy could be rewritten from inside the isolation and the mount items that could be
//! redirected from there (specification section 5.6), and the rules about the work place
//! and the current directory (section 6.5). Pure.

use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, Warning};
use crate::environment::HomeDirectory;
use crate::layers::{Directive, Layer, LayerOrigin};
use crate::mounts::{byte_order, ExpandedPolicy, MountFacts, ResolvedItem, ResolvedMounts};
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

/// One written mount item by its expanded path, with the form it was written in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenItem {
    pub directive: Directive,
    pub written: String,
    pub path: PathBuf,
}

/// The paths the rule of specification section 5.6 on written mount items applies to:
/// every written item that expanded to a path (any layer, the command line included), and
/// the `--workspace` path as given. The generated items of section 6.3 are not written,
/// and the current directory standing in for an omitted `--workspace` is not given.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WrittenPaths {
    pub items: Vec<WrittenItem>,
    pub workspace: Option<PathBuf>,
}

pub fn written_paths(expanded: &ExpandedPolicy, workspace: Option<&Path>) -> WrittenPaths {
    WrittenPaths {
        items: expanded
            .mounts
            .iter()
            .filter_map(|item| {
                Some(WrittenItem {
                    directive: item.directive,
                    written: item.written.to_string(),
                    path: item.path.path()?.to_path_buf(),
                })
            })
            .collect(),
        workspace: workspace.map(Path::to_path_buf),
    }
}

/// Runs the checks in the order of specification section 13 and returns the warnings of
/// a run that may go on.
pub fn check_placement(
    resolved: &ResolvedMounts,
    protected: &ProtectedPaths,
    written: &WrittenPaths,
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
    check_written_paths(written, &writable, variables, facts)?;
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

/// A written item or the `--workspace` whose resolution consulted something inside a
/// writable item must itself resolve inside a writable item (specification section 5.6):
/// what it consulted could be re-pointed from inside the isolation, and the next start
/// would apply the directive to any host path. The items are taken in the order of
/// section 6.4, then the workspace. An item that resolves to nothing has nothing to bind.
fn check_written_paths(
    written: &WrittenPaths,
    writable: &[&ResolvedItem],
    variables: &Variables,
    facts: &MountFacts,
) -> Result<(), Diagnostic> {
    let mut items: Vec<(&WrittenItem, PathBuf)> = written
        .items
        .iter()
        .filter_map(|item| Some((item, facts.entry(&item.path).path()?.to_path_buf())))
        .collect();
    items.sort_by(|(_, a), (_, b)| byte_order(a, b));
    for (item, real) in items {
        let role = format!(
            "the `{}` item `{}` at {}",
            directive_name(item.directive),
            item.written,
            item.path.display()
        );
        check_landing(&role, &item.path, &real, written, writable, facts)?;
    }
    if let Some(workspace) = &written.workspace {
        let role = format!("the workspace {}", workspace.display());
        check_landing(
            &role,
            workspace,
            &variables.workspace,
            written,
            writable,
            facts,
        )?;
    }
    Ok(())
}

/// The rule of `check_written_paths` for one path: `given` resolved to `real`.
fn check_landing(
    role: &str,
    given: &Path,
    real: &Path,
    written: &WrittenPaths,
    writable: &[&ResolvedItem],
    facts: &MountFacts,
) -> Result<(), Diagnostic> {
    let consulted = facts
        .traversed_links(given)
        .iter()
        .chain(facts.visited_directories(given))
        .find_map(|place| writable.iter().find(|item| place.starts_with(&item.real)));
    let Some(consulted) = consulted else {
        return Ok(());
    };
    if lands_in_writable(given, real, written, writable, facts) {
        return Ok(());
    }
    Err(Diagnostic::path(format!(
        "{role} resolves to {} through the `{}` item {} but outside every `rw` and \
         `rw-file` item, so it could be redirected from inside the isolation",
        real.display(),
        directive_name(consulted.directive),
        consulted.real.display()
    )))
}

/// Whether `real`, what `given` resolved to, is inside a writable item (the same or a
/// descendant). A writable item at `real` itself counts only when another written writable
/// item, at a different given path, resolves there too: the one at `given` is the item
/// under check, and landing on its own resolved place proves nothing.
fn lands_in_writable(
    given: &Path,
    real: &Path,
    written: &WrittenPaths,
    writable: &[&ResolvedItem],
    facts: &MountFacts,
) -> bool {
    let inside_another = writable
        .iter()
        .any(|item| item.real != real && real.starts_with(&item.real));
    let same_as_another = writable.iter().any(|item| item.real == real)
        && written.items.iter().any(|other| {
            matches!(other.directive, Directive::Rw | Directive::RwFile)
                && other.path != given
                && facts.entry(&other.path).path() == Some(real)
        });
    inside_another || same_as_another
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

/// Resolving `path` must consult nothing inside a writable item (specification section
/// 5.6): every prefix of `path` (itself and each ancestor) that exists must not have its
/// real path inside one, no symbolic link the resolution passes through may itself sit
/// inside one, and no directory the resolution passes through may be inside one. Any of
/// them could be re-pointed or replaced from inside the isolation. The links and
/// directories are not all prefixes of `path`: a link's target may pass through further
/// links, or through a directory it leaves again with `..`.
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
    for directory in facts.visited_directories(path) {
        if let Some(item) = writable
            .iter()
            .find(|item| directory.starts_with(&item.real))
        {
            return Err(Diagnostic::path(format!(
                "the directory {} on the way to {role} {} is inside the `{}` item {} and its \
                 entries could be replaced from inside the isolation",
                directory.display(),
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
