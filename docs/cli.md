# Command line

```
process-wrap [OPTIONS] -- COMMAND [ARGS]...
process-wrap [OPTIONS] --print-plan [-- COMMAND [ARGS]...]
process-wrap init [NAME]
process-wrap --version
process-wrap --help
```

## Options

| Option | Meaning |
| --- | --- |
| `--profile NAME` | The profile for the global scope: `$XDG_CONFIG_HOME/process-wrap/profile/NAME.toml` (or `~/.config/process-wrap/profile/NAME.toml`). Defaults to `default`. |
| `--policy-file PATH` | A policy file for the process scope, merged on top of the profile. |
| `--workspace PATH` | The workspace. Defaults to the current directory. |
| `--rw PATH` | An `rw` directive on the command-line layer. Repeatable. |
| `--hide PATH` | A `hide` directive on the command-line layer. Repeatable. |
| `--print-plan` | Print the plan and exit without running the command. |
| `--version`, `--help` | Print the version or the usage. Each is used alone. |
| `init [NAME]` | Write the built-in default to `profile/NAME.toml` (`default` when `NAME` is left out), print its path, and exit. Used alone; see [Getting started](getting-started.md#write-the-boundary-out-and-edit-it). |

Everything after `--` is the command and its arguments, passed through unchanged. `process-wrap`
never rewrites them, and the command sees the name it was given as its `argv[0]` (`sh`, not
`/usr/bin/sh`), inside the isolation and when nested.

Relative paths given on the command line are taken from the current directory; `~` and variables
are not expanded there. Options that take a value accept `--opt VALUE` and `--opt=VALUE`; an
empty value, or a value starting with `-` in the separated form, is a usage error. Options other
than `--rw` and `--hide` can be given once.

A typical launch:

```sh
cd ~/work/project
process-wrap -- codex
```

## The plan

To see what would happen without running anything:

```sh
process-wrap --print-plan -- codex
```

The plan shows:

- the merged policy and the policy files read;
- the four variables (`${workspace}`, `${worktree}`, `${git_common_dir}`, `${config_dir}`);
- every mount item, applied or skipped with the reason;
- every scan hit left visible, and every scan root, `hide-mounts` `under`, or `path-prepend`
  entry skipped, each with the reason;
- the final environment, with secret values masked;
- the resolved command and the `bwrap` argument list.

Only the values of the variables the policy names under `secrets` are masked. Every other
variable of the final environment is printed with its value as it is, and with
`env.mode = "inherit"` that includes any host credential whose name matches none of the `unset`
patterns ([known gap 6](security.md#known-gaps)). Treat the output of `--print-plan` as
sensitive.

## Exit codes and diagnostics

A failure of `process-wrap` itself is one line on standard error of the form
`process-wrap: <kind>: <description>`, and the exit code is 125. Two exceptions: a command that
cannot be found exits 127, and, in a nested run, a command that was found but cannot be executed
(a script whose interpreter does not exist, a file of a format the kernel cannot run) exits 126.

The kinds are `usage`, `policy`, `path`, `secret`, `env`, `bwrap`, `command not found`, and
`command not executable`. Warnings are one line each starting with `process-wrap: warning: ` and
do not stop the run.

When the command runs, its exit code is returned as it is; a command killed by signal `s` yields
128 + `s`. `process-wrap` executes `bwrap` in place rather than waiting for it as a child, so a
failure of `bwrap` itself (a mount that cannot be made, an `exec` that fails) shows as `bwrap`'s
own output and exit code. Only in a nested run, where `process-wrap` executes the command itself,
does a failed `exec` become the `command not executable` diagnostic above.

## Nesting

`process-wrap` sets `PROCESS_WRAP=1` inside the isolation. When it finds that variable already
set, it does not isolate again: it prints a nesting warning and executes the command itself,
without `bwrap`. Nesting is detected only through that variable
([known gap 8](security.md#known-gaps)).

## Open files

Before making the file descriptors it hands to `bwrap` (one per hidden file, plus the seccomp
filter), `process-wrap` raises its soft limit on open files to the hard limit, always, so that a
scan hiding thousands of files starts under the usual limit of 1024 and the same input gives the
same result. The command inherits the raised limit. A nested run makes no descriptors and leaves
the limit alone.
