# Changelog

## Unreleased

## [0.1.0] - Unreleased

### Added

- Added `process-wrap`, which runs a command inside a bubblewrap mount namespace shaped by a
  layered policy (profile, `--policy-file`, command line) and returns the command's exit code
  unchanged. Policies name `rw`, `rw-file`, `ro`, and `hide` mount items with `~` and the
  variables `${workspace}`, `${worktree}`, `${git_common_dir}`, and `${config_dir}`; scan the
  worktree for environment files to hide; hide mounts by file system type; choose the network
  mode; shape the environment; inject secrets from files; and add git URL rewrites.
- Added the placement checks that refuse a policy file, the configuration directory, a secret
  file, or a `path-prepend` entry that could be swapped from inside the isolation; a written
  `rw`, `rw-file`, or `hide` item, a `--workspace`, a scan `root`, or a `hide-mounts` `under`
  whose path resolves through an `rw` item to a place outside every root item; a `hide` whose
  path follows a link inside an `rw` item, wherever it lands; a scan `root` or `hide-mounts`
  `under` below an `rw` mount point; an item that would expose what a lower `hide` or `ro`
  covers, including the `hide` items generated for `secrets/` and hidden mounts; and an item
  landing on `/`, `/dev`, or `/proc` or inside the latter two. Written `ro` items are not held
  to the root-item check, so a dotfiles link written as `ro` passes. The home directory and its
  ancestors are refused as a worktree or an `rw` item.
- Added a seccomp filter that fails `ioctl(TIOCSTI)` with `EPERM` and ends processes making
  foreign-architecture or x32 system calls.
- Added `--print-plan`, one-line diagnostics with the exit codes 125 (a diagnostic of
  `process-wrap` itself), 126 (a command found but not executable, in a nested run), and 127 (a
  command not found), nesting detection through `PROCESS_WRAP=1`, and the bundled WSL2 profile
  `examples/profile/default.toml`.
- The command sees the `COMMAND` string it was given as its `argv[0]`, inside the isolation
  (`bwrap --argv0`) and when nested, so multi-call binaries behave as typed.
- The soft limit on open files is raised to the hard limit before the descriptors for `bwrap`
  are made, so a scan hiding more files than the usual limit of 1024 still starts; the command
  inherits the raised limit, and a nested run leaves it unchanged.
- A secret file's trailing LF or CR LF is removed as one newline.
- A scan hit that is a symbolic link into an `ro` item is left visible with the reason shown in
  the plan, or stops the launch with `path` when that `ro` item could be re-pointed from inside;
  a `hide-mounts` with an unreadable mount list stops with `path`; skipped scan roots,
  `hide-mounts` `under`s, and `path-prepend` entries are shown in the plan with the reason.
