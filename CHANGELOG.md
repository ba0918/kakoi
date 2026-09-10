# Changelog

## Unreleased

### Changed

- **Breaking**: the project is renamed from `process-wrap` to `kakoi`. The executable, the
  environment variables (`KAKOI` and `KAKOI_SHIM_OFF`), the configuration directory
  (`$XDG_CONFIG_HOME/kakoi/`), the shared directory (`/tmp/kakoi`), the diagnostic prefix, the setup
  skill (`kakoi-setup`), the release archive
  (`kakoi-v<version>-x86_64-unknown-linux-musl.tar.gz`), and the repository URL all use the new
  name. There is no compatibility path from the old names: an existing configuration directory,
  shim, or environment variable must be moved to the new names by hand.

## [0.1.1] - 2026-09-09

### Added

- Every release carries a statically linked binary for Linux on x86_64, as
  `process-wrap-v<version>-x86_64-unknown-linux-musl.tar.gz` with its SHA-256 beside it.
  `mise use -g github:ba0918/process-wrap` installs it, and the archive can be taken by hand
  from the release page. Installing no longer needs a Rust toolchain, and the binary links
  against nothing on the machine.

### Changed

- The install instructions lead with that binary. Building from source is still supported and
  is now `cargo install --git https://github.com/ba0918/process-wrap --locked`; the old
  `cargo install --path .` needed a clone.
- The setup skill carries its own copy of the shim template, so it no longer asks where the
  `process-wrap` source tree is. The shim template itself can be fetched from the repository
  with `curl` rather than copied out of a clone.

## [0.1.0] - 2026-09-09

The first release. `process-wrap` runs a command inside a bubblewrap mount namespace shaped by a
layered policy, and returns the command's exit code unchanged.

### Added

- A policy merged from three layers — a profile, `--policy-file`, and the command line. It names
  `rw`, `rw-file`, `ro`, and `hide` mount items with `~` and the variables `${workspace}`,
  `${worktree}`, `${git_common_dir}`, and `${config_dir}`; scans the worktree for environment
  files to hide; hides mounts by file system type; chooses the network mode; shapes the
  environment; injects secrets from files; and adds git URL rewrites.
- Placement checks that refuse a policy which could be subverted from inside the isolation: a
  policy file, configuration directory, secret file, or `path-prepend` entry that could be
  swapped; a path resolving through a writable item to somewhere outside every root item, or
  following a link inside one; an item that would expose what a lower `hide` or `ro` covers; and
  an item landing on `/`, `/dev`, or `/proc`. The home directory and its ancestors are refused as
  a worktree or an `rw` item.
- A seccomp filter that fails `ioctl(TIOCSTI)` with `EPERM`, so keystrokes cannot be pushed into
  the terminal that way, and ends processes making foreign-architecture or x32 system calls.
- `--print-plan`, which shows what a launch would do without launching: a summary for reading,
  `=full` for the merged policy, the origin of every item, and the `bwrap` argument list, and
  `=json` for tools and agents, whose keys are a contract.
- One-line diagnostics with the exit codes 125 (`process-wrap` itself), 126 (a command found but
  not executable, in a nested run), and 127 (a command not found). The command's own exit code is
  returned unchanged, and a nested run, marked by `PROCESS_WRAP=1`, executes the command without
  isolating it again.
- The bundled WSL2 profile compiled into the binary as the built-in default, so `process-wrap --
  COMMAND` works with nothing written to the configuration directory.
- `process-wrap init [NAME]`, which writes that profile to `profile/NAME.toml` and creates the
  configuration directory, `profile/`, and `secrets/` (mode 0700). It refuses to replace anything
  already at that name and has no `--force`. It is the only form that writes.
- `examples/shim/codex`, a shim template that sends every invocation through `process-wrap`,
  split into a body that does not depend on the command being wrapped and a tool section at the
  top of the file that carries everything that does.
- `skills/process-wrap-setup/SKILL.md`, an Agent Skill that proposes the machine-specific parts
  of a profile and a shim with its tool section filled in, shows a diff, and waits for approval
  before writing.

[0.1.1]: https://github.com/ba0918/process-wrap/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ba0918/process-wrap/releases/tag/v0.1.0
