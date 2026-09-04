//! The checks on where things are placed, made on the resolved mount items: the places a
//! policy could be rewritten from inside the isolation and the mount items that could be
//! redirected from there (specification section 5.6), and the rules about the work place
//! and the current directory (section 6.5). Pure.

use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, Warning};
use crate::environment::HomeDirectory;
use crate::layers::{Directive, Layer, LayerOrigin};
use crate::mounts::{byte_order, ExpandedPolicy, MountFacts, ResolvedItem, ResolvedMounts};
use crate::policy::{PolicyPath, Variable};
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

/// One written mount item by its expanded path, with the form it was written in and the
/// variable that form starts with, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenItem {
    pub directive: Directive,
    pub written: String,
    pub variable: Option<Variable>,
    pub path: PathBuf,
}

impl WrittenItem {
    /// Whether the item was expanded from a variable derived from the workspace, so that
    /// its resolution inherits what the workspace's resolution referenced (specification
    /// section 5.6).
    fn inherits_from_workspace(&self) -> bool {
        matches!(
            self.variable,
            Some(Variable::Workspace | Variable::Worktree | Variable::GitCommonDir)
        )
    }
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
                    variable: match item.written {
                        PolicyPath::Variable(variable, _) => Some(variable),
                        PolicyPath::Absolute(_) | PolicyPath::Home(_) => None,
                    },
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
    check_written_paths(written, &writable, variables, current_dir, facts)?;
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

/// The root-item check of specification section 5.6: a written item or the `--workspace`
/// whose resolution referenced something inside a writable item must itself resolve inside
/// a root item, since what it referenced could be re-pointed from inside the isolation and
/// the next start would apply the directive to any host path. The items are taken in the
/// order of section 6.4, then the workspace. An item that resolves to nothing has nothing
/// to bind. A `--workspace` whose real path is the current directory is exempt and hands
/// nothing down: the process already sits there, so no redirection can move it.
fn check_written_paths(
    written: &WrittenPaths,
    writable: &[&ResolvedItem],
    variables: &Variables,
    current_dir: &Path,
    facts: &MountFacts,
) -> Result<(), Diagnostic> {
    let workspace = written
        .workspace
        .as_deref()
        .filter(|_| variables.workspace != current_dir);
    let roots = root_items(written, workspace, writable, facts);
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
        let reference = referenced_writable(item, workspace, writable, facts);
        check_landing(&role, reference, &real, &roots)?;
    }
    if let Some(workspace) = workspace {
        let role = format!("the workspace {}", workspace.display());
        let reference = consulted_writable(workspace, writable, facts).map(Reference::own);
        check_landing(&role, reference, &variables.workspace, &roots)?;
        check_workspace_links(workspace, &variables.workspace, &roots, written, facts)?;
    }
    Ok(())
}

/// The further rule of specification section 5.6 for a `--workspace` under check whose
/// resolution followed a symbolic link: its real path must lie inside a root item with a
/// written form not expanded from the workspace itself. The writable set is derived from
/// the workspace, so a redirected workspace makes the place that was writable before the
/// redirection (the worktree of `rw = ["${worktree}"]`) invisible to the other rules, and
/// a link is the only means of redirection out of a writable item. The first link followed
/// is the one named.
fn check_workspace_links(
    workspace: &Path,
    real: &Path,
    roots: &[&ResolvedItem],
    written: &WrittenPaths,
    facts: &MountFacts,
) -> Result<(), Diagnostic> {
    let Some(link) = facts.traversed_links(workspace).first() else {
        return Ok(());
    };
    let anchored = roots.iter().any(|root| {
        real.starts_with(&root.real)
            && writable_forms_of(root, written, facts).any(|form| !form.inherits_from_workspace())
    });
    if anchored {
        return Ok(());
    }
    Err(Diagnostic::path(format!(
        "the workspace {} resolves to {} through the symbolic link {} but outside every `rw` \
         and `rw-file` item not expanded from the workspace itself, so it could be redirected \
         from inside the isolation",
        workspace.display(),
        real.display(),
        link.display()
    )))
}

/// The root items of specification section 2: the writable items none of whose written
/// forms (the written writable items resolving to the same real path, section 5.4)
/// referenced anything inside a writable item, the item itself included.
fn root_items<'a>(
    written: &WrittenPaths,
    workspace: Option<&Path>,
    writable: &'a [&'a ResolvedItem],
    facts: &MountFacts,
) -> Vec<&'a ResolvedItem> {
    writable
        .iter()
        .filter(|item| {
            writable_forms_of(item, written, facts)
                .all(|form| referenced_writable(form, workspace, writable, facts).is_none())
        })
        .copied()
        .collect()
}

/// The written writable forms (`rw` or `rw-file`, specification section 5.4) that resolve
/// to `item`.
fn writable_forms_of<'a>(
    item: &'a ResolvedItem,
    written: &'a WrittenPaths,
    facts: &'a MountFacts,
) -> impl Iterator<Item = &'a WrittenItem> {
    written.items.iter().filter(move |form| {
        matches!(form.directive, Directive::Rw | Directive::RwFile)
            && facts.entry(&form.path).path() == Some(item.real.as_path())
    })
}

/// A writable item the resolution of a path under check referenced.
#[derive(Debug, Clone, Copy)]
struct Reference<'a> {
    item: &'a ResolvedItem,
    /// Whether it was inherited from the `--workspace` rather than referenced by the
    /// path's own resolution.
    inherited: bool,
}

impl<'a> Reference<'a> {
    fn own(item: &'a ResolvedItem) -> Self {
        Self {
            item,
            inherited: false,
        }
    }
}

/// The first writable item that resolving `item` referenced: what its own resolution
/// consulted, or, for an item expanded from a workspace variable, what the resolution of
/// `workspace` (the `--workspace` under check, if any) consulted (specification
/// section 5.6).
fn referenced_writable<'a>(
    item: &WrittenItem,
    workspace: Option<&Path>,
    writable: &'a [&'a ResolvedItem],
    facts: &MountFacts,
) -> Option<Reference<'a>> {
    consulted_writable(&item.path, writable, facts)
        .map(Reference::own)
        .or_else(|| {
            workspace
                .filter(|_| item.inherits_from_workspace())
                .and_then(|workspace| consulted_writable(workspace, writable, facts))
                .map(|item| Reference {
                    item,
                    inherited: true,
                })
        })
}

/// The rule of `check_written_paths` for one path that resolved to `real` and referenced
/// `reference`, if anything writable: `real` must be a root item or inside one.
fn check_landing(
    role: &str,
    reference: Option<Reference>,
    real: &Path,
    roots: &[&ResolvedItem],
) -> Result<(), Diagnostic> {
    let Some(reference) = reference else {
        return Ok(());
    };
    if roots.iter().any(|root| real.starts_with(&root.real)) {
        return Ok(());
    }
    let through = format!(
        "through the `{}` item {}",
        directive_name(reference.item.directive),
        reference.item.real.display()
    );
    let how = if reference.inherited {
        format!("and the workspace it was expanded from resolved {through},")
    } else {
        through
    };
    Err(Diagnostic::path(format!(
        "{role} resolves to {} {how} but outside every `rw` and `rw-file` item that could \
         not itself be redirected, so it could be redirected from inside the isolation",
        real.display()
    )))
}

/// The first writable item that resolving `given` consulted: one holding a link the
/// resolution followed or a directory it passed through.
fn consulted_writable<'a>(
    given: &Path,
    writable: &'a [&'a ResolvedItem],
    facts: &MountFacts,
) -> Option<&'a ResolvedItem> {
    facts
        .traversed_links(given)
        .iter()
        .chain(facts.visited_directories(given))
        .find_map(|place| writable.iter().find(|item| place.starts_with(&item.real)))
        .copied()
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
