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
  file, a `path-prepend` entry, or a redirected mount item that could be swapped from inside the
  isolation, and the refusal of the home directory and its ancestors as a worktree or an `rw`
  item.
- Added a seccomp filter that fails `ioctl(TIOCSTI)` with `EPERM` and ends processes making
  foreign-architecture or x32 system calls.
- Added `--print-plan`, one-line diagnostics with the exit codes 125 and 127, nesting detection
  through `PROCESS_WRAP=1`, and the bundled WSL2 profile `examples/profile/default.toml`.
