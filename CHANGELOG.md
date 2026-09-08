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
  whose path resolves through an `rw` item to a place outside every root item; a `hide`, a scan
  `root`, or a `hide-mounts` `under` whose path follows a link inside an `rw` item, wherever it
  lands; a scan `root` or `hide-mounts` `under` below an `rw` mount point; an item that would
  expose what a lower `hide` or `ro` covers, including the `hide` items generated for `secrets/`
  and hidden mounts; and an item landing on `/`, `/dev`, or `/proc` or inside the latter two.
  Written `ro` items are not held to the root-item check, so a dotfiles link written as `ro`
  passes. The home directory and its ancestors are refused as a worktree or an `rw` item.
- Added a seccomp filter that fails `ioctl(TIOCSTI)` with `EPERM` and ends processes making
  foreign-architecture or x32 system calls.
- Added `--print-plan`, a summary of the plan for reading (the mount items with the home
  directory as `~`, the environment as its difference from the host's, secret values masked),
  `--print-plan=full`, which adds the merged policy, the origin of every item, the whole
  environment, and the `bwrap` argument list, and `--print-plan=json`, the same content as one
  JSON document whose keys are a contract; one-line diagnostics with the exit codes 125 (a diagnostic of
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
- The bundled profile is compiled into the binary as the built-in default, so `process-wrap --
  COMMAND` works with nothing written to the configuration directory. It stands in for the global
  scope only when `--profile` is `default` and `profile/default.toml` is not there at all; a
  broken link or a regular file in the way is a `policy` diagnostic instead, and a named profile
  never falls back. The plan names it with the string `process-wrap init`.
- Added `process-wrap init [NAME]`, which writes the built-in default to `profile/NAME.toml`
  (`default` when the name is left out), creating the configuration directory, its missing
  ancestors, `profile/`, and `secrets/` (mode 0700, narrowing a `secrets/` that is already there
  to that mode), and printing the path it wrote. It refuses
  to replace anything already at that name and has no `--force`. `NAME` is one path component
  without a control character, the constraint `--profile` carries too, so the path it prints
  stays the one line it is documented to be. It is the only form that writes,
  and it checks the grammar and the home directory only, so it works without `bwrap` and inside
  an isolation.
- A configuration directory that does not exist leaves `${config_dir}` without a value, so items
  written with it are skipped with the reason shown in the plan; the placement checks still hold
  it to its deepest existing ancestor, so a launch whose configuration directory is named under a
  writable item stops with `path` rather than letting a profile be planted there; one that exists
  but cannot be followed to a directory is a `path` diagnostic.
- Added `examples/shim/codex`, a shim template made of a body that does not depend on the command
  being wrapped and a tool section at the top of the file that carries everything that does: the
  real command's name, the flag that turns its own sandbox off, the options copied to
  `--workspace` and `--rw`, and two lists that ship empty. Every invocation goes through
  `process-wrap` by default, `--help` and `--version` included, and no subcommand decides
  otherwise; a subcommand in the allow list is passed straight to the real command instead, as is
  every invocation under `PROCESS_WRAP_SHIM_OFF=1`, and neither creates `/tmp/process-wrap`. The
  values in the tool section are filled in for codex as an example of a command to wrap.
- Added `skills/process-wrap-setup/SKILL.md`, an Agent Skill that proposes the machine-specific
  parts of a profile, a copy of the shim template with its tool section filled in from the wrapped
  command's own `--help`, `path-prepend` entries for replacement commands, and where a command
  that broke inside the isolation belongs, and that shows a diff and waits for approval before
  writing.
- The bundled profile's `secrets` entry is commented out, so a start on the built-in default does
  not warn about a secret file nobody has placed.
