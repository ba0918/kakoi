//! Symbolic bwrap argument generation from pure isolation decisions.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::command_limits::{AllowedList, CommandLimits, ALLOWED_LIST, FIRST_PROCESS, FIRST_ROOT};
use crate::copies::{CopiedEntry, CopySource, CopySources, FileContent};
use crate::guard_placement::{GuardPlan, GUARD_LOCATION, GUARD_ROOT, GUARD_TABLE};
use crate::layers::Directive;
use crate::listed::ListedRoot;
use crate::mounts::{EntryKind, ResolvedItem};
use crate::plan::{Argument, LaunchLayout, Provisions, ResolvedCommand, NESTING_MARK, TUN_DEVICE};
use crate::policy::NetworkMode;
use crate::shared_files::SharedFile;

fn put(
    provisions: &Provisions,
    file: SharedFile,
    destination: impl Into<OsString>,
) -> [Argument; 3] {
    match &provisions.shared_files {
        Some(place) => [
            text("--ro-bind"),
            Argument::SharedFile {
                file,
                path: place.join(file.name()),
            },
            text(destination),
        ],
        None => [text("--ro-bind-data"), file.from_data(), text(destination)],
    }
}

fn text(text: impl Into<OsString>) -> Argument {
    Argument::Literal(text.into())
}

/// Fixed arguments, resolved mount order, then the command after a separator.
pub fn bwrap_arguments(
    network_mode: NetworkMode,
    current_dir: &Path,
    items: &[ResolvedItem],
    copies: &CopySources,
    guards: &GuardPlan,
    provisions: &Provisions,
    command: Option<&ResolvedCommand>,
) -> Vec<Argument> {
    bwrap_arguments_with_layout(
        network_mode,
        current_dir,
        items,
        copies,
        guards,
        provisions,
        command,
    )
    .0
}

/// Records replacement locations while generating, rather than parsing argument text.
pub fn bwrap_arguments_with_layout(
    network_mode: NetworkMode,
    current_dir: &Path,
    items: &[ResolvedItem],
    copies: &CopySources,
    guards: &GuardPlan,
    provisions: &Provisions,
    command: Option<&ResolvedCommand>,
) -> (Vec<Argument>, LaunchLayout) {
    let mut layout = LaunchLayout::default();
    let mut arguments = match provisions.listed {
        Some(_) => vec![text("--tmpfs"), text("/")],
        None => vec![text("--ro-bind"), text("/"), text("/")],
    };
    arguments.extend([
        text("--dev"),
        text("/dev"),
        text("--ro-bind-data"),
        Argument::EmptyFile,
        text(NESTING_MARK),
    ]);
    if provisions.tun {
        arguments.extend([text("--dev-bind"), text(TUN_DEVICE), text(TUN_DEVICE)]);
    }
    if provisions.outer_guard {
        arguments.extend([text("--ro-bind"), text(GUARD_ROOT), text(GUARD_ROOT)]);
    }
    arguments.extend([text("--proc"), text("/proc"), text("--unshare-all")]);
    if matches!(network_mode, NetworkMode::Host | NetworkMode::Filtered) {
        arguments.push(text("--share-net"));
    }
    if network_mode == NetworkMode::Filtered {
        arguments.extend([text("--cap-drop"), text("ALL")]);
    }
    arguments.extend([
        text("--die-with-parent"),
        text("--chdir"),
        text(current_dir),
        text("--seccomp"),
        Argument::Seccomp,
    ]);
    if let Some(command) = command {
        layout.argv0 = Some(arguments.len() + 1);
        arguments.extend([text("--argv0"), text(command.command.as_os_str())]);
    }
    if let Some(root) = &provisions.listed {
        arguments.extend(listed_arguments(root));
    }
    for item in items {
        let real = text(item.real.as_os_str());
        match (item.directive, item.kind) {
            (Directive::Rw | Directive::RwFile, _) => {
                arguments.extend([text("--bind"), real.clone(), real])
            }
            (Directive::Ro, _) => arguments.extend([text("--ro-bind"), real.clone(), real]),
            (Directive::RwCopy, _) => {
                arguments.extend(copy_arguments(item, copies.sources.get(&item.real)))
            }
            (Directive::Hide, EntryKind::Directory) => arguments.extend([text("--tmpfs"), real]),
            (Directive::Hide, EntryKind::NotDirectory) => {
                arguments.extend(put(provisions, SharedFile::Empty, item.real.as_os_str()))
            }
        }
    }
    if network_mode == NetworkMode::Filtered {
        // User mounts must not replace the managed resolver configuration.
        let destination = provisions
            .listed
            .as_ref()
            .and_then(|root| root.filtered_resolver.as_deref())
            .unwrap_or(Path::new("/etc/resolv.conf"));
        arguments.extend(put(
            provisions,
            SharedFile::Resolver,
            destination.as_os_str(),
        ));
        layout.resolver_destination = Some(arguments.len() - 1);
    }
    arguments.extend(guard_arguments(guards, provisions.commands.as_ref()));
    if provisions.listed.is_some() {
        arguments.extend([text("--remount-ro"), text("/")]);
    }
    if let Some(command) = command {
        layout.command_separator = Some(arguments.len());
        arguments.push(text("--"));
        if provisions.commands.is_some() {
            arguments.push(text(FIRST_PROCESS));
        }
        arguments.push(text(command.path.as_os_str()));
        arguments.extend(command.arguments.iter().map(text));
    }
    (arguments, layout)
}

fn listed_arguments(root: &ListedRoot) -> Vec<Argument> {
    let mut arguments = vec![text("--perms"), text("1777"), text("--tmpfs"), text("/tmp")];
    for directory in &root.directories {
        arguments.extend([
            text("--perms"),
            text("0755"),
            text("--dir"),
            text(directory.as_os_str()),
        ]);
    }
    for directory in &root.base {
        arguments.extend(read_only_bind(directory, directory));
    }
    for link in &root.links {
        arguments.extend([
            text("--symlink"),
            text(link.target.as_os_str()),
            text(link.place.as_os_str()),
        ]);
    }
    if let Some(resolver) = &root.resolver {
        arguments.extend(read_only_bind(resolver, resolver));
    }
    arguments
}

fn copy_arguments(item: &ResolvedItem, source: Option<&CopySource>) -> Vec<Argument> {
    let real = || text(item.real.as_os_str());
    match (source, item.kind) {
        // --file would write through to the host when an rw parent covers the file.
        (Some(CopySource::File { mode, content }), _) => vec![
            text("--perms"),
            text(octal(*mode)),
            text("--bind-data"),
            Argument::CopiedFile(content.clone()),
            real(),
        ],
        (Some(CopySource::Directory { mode, entries }), _) => {
            let mut arguments = vec![text("--perms"), text(octal(*mode)), text("--tmpfs"), real()];
            for entry in entries {
                let destination = text(item.real.join(entry.relative()).into_os_string());
                match entry {
                    CopiedEntry::Directory { mode, .. } => arguments.extend([
                        text("--perms"),
                        text(octal(*mode)),
                        text("--dir"),
                        destination,
                    ]),
                    CopiedEntry::File { mode, content, .. } => arguments.extend([
                        text("--perms"),
                        text(octal(*mode)),
                        text("--file"),
                        Argument::CopiedFile(content.clone()),
                        destination,
                    ]),
                    CopiedEntry::Symlink { target, .. } => {
                        arguments.extend([text("--symlink"), text(target.as_os_str()), destination])
                    }
                }
            }
            arguments
        }
        // Missing copy facts fail closed rather than exposing the host's content.
        (None, EntryKind::Directory) => vec![text("--tmpfs"), real()],
        (None, EntryKind::NotDirectory) => {
            vec![text("--ro-bind-data"), Argument::EmptyFile, real()]
        }
    }
}

fn guard_arguments(guards: &GuardPlan, commands: Option<&CommandLimits>) -> Vec<Argument> {
    let mut arguments = Vec::new();
    if let Some(kakoi) = guards
        .executable
        .as_deref()
        .filter(|_| !guards.placed.is_empty())
    {
        arguments.extend(guard_tmpfs(kakoi, guards));
    }
    if let Some(limits) = commands {
        arguments.extend(first_process_tmpfs(limits));
    }
    arguments
}

fn guard_tmpfs(kakoi: &Path, guards: &GuardPlan) -> Vec<Argument> {
    let mut arguments = Vec::from(tmpfs(GUARD_ROOT));
    arguments.extend(directory(Path::new(GUARD_LOCATION)));
    for guard in &guards.placed {
        arguments.extend(read_only_bind(kakoi, &guard.guard()));
    }
    for (real, relocated) in &guards.overlaid {
        // bwrap's implicit directories would be readable only by their owner.
        let parent = relocated
            .parent()
            .expect("a relocated program is in a directory");
        for directory_path in [parent.parent(), Some(parent)].into_iter().flatten() {
            arguments.extend(directory(directory_path));
        }
        arguments.extend(read_only_bind(real, relocated));
        arguments.extend(read_only_bind(kakoi, real));
    }
    arguments.extend(read_only_data(guards.table.to_bytes(), GUARD_TABLE));
    arguments.extend(remount_read_only(GUARD_ROOT));
    arguments
}

fn first_process_tmpfs(limits: &CommandLimits) -> Vec<Argument> {
    let mut arguments = Vec::from(tmpfs(FIRST_ROOT));
    arguments.extend(read_only_bind(&limits.executable, Path::new(FIRST_PROCESS)));
    let allowed: Vec<PathBuf> = limits
        .allowed
        .iter()
        .chain(&limits.relocated)
        .chain(&limits.outer_guards)
        .cloned()
        .collect();
    arguments.extend(read_only_data(
        AllowedList::new(&allowed).to_bytes(),
        ALLOWED_LIST,
    ));
    arguments.extend(remount_read_only(FIRST_ROOT));
    arguments
}

fn tmpfs(path: &str) -> [Argument; 4] {
    [text("--perms"), text("0755"), text("--tmpfs"), text(path)]
}
fn directory(path: &Path) -> [Argument; 4] {
    [
        text("--perms"),
        text("0755"),
        text("--dir"),
        text(path.as_os_str()),
    ]
}
fn read_only_bind(from: &Path, to: &Path) -> [Argument; 3] {
    [
        text("--ro-bind"),
        text(from.as_os_str()),
        text(to.as_os_str()),
    ]
}
fn read_only_data(content: Vec<u8>, to: &str) -> [Argument; 5] {
    [
        text("--perms"),
        text("0444"),
        text("--ro-bind-data"),
        Argument::CopiedFile(FileContent::new(content)),
        text(to),
    ]
}
fn remount_read_only(path: &str) -> [Argument; 2] {
    [text("--remount-ro"), text(path)]
}
fn octal(mode: u32) -> String {
    format!("{mode:04o}")
}
