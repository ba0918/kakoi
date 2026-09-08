# Command line

```
process-wrap [OPTIONS] -- COMMAND [ARGS]...
process-wrap [OPTIONS] --print-plan[=FORM] [-- COMMAND [ARGS]...]
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
| `--print-plan[=FORM]` | Print the plan and exit without running the command. `FORM` is `summary` (the default), `full`, or `json`; see [The plan](#the-plan). The value is written with `=` only. |
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

The plan comes in three forms. The summary, which `--print-plan` prints by itself, is written
to be read:

```
policy files:
  /home/you/.config/process-wrap/profile/default.toml
variables:
  workspace = /home/you/work/project
  worktree = /home/you/work/project
  git_common_dir = /home/you/work/project/.git
  config_dir = /home/you/.config/process-wrap
network: host
mounts (~ is /home/you):
  hide    ~/.ssh
  rw      ~/work/project
  hide    ~/work/project/.env (scan)
  skipped rw `~/.npm`: does not exist
environment (inherit): 41 variables as on the host, and:
  unset  SSH_AUTH_SOCK
  set    PATH=/home/you/.local/lib/process-wrap/bin:<the host's PATH>
  set    PROCESS_WRAP=1
  secret GH_TOKEN (value not shown)
command: /usr/bin/codex
bwrap: /usr/bin/bwrap
```

It shows:

- the policy files read;
- the four variables (`${workspace}`, `${worktree}`, `${git_common_dir}`, `${config_dir}`);
- the network mode;
- every mount item, applied or skipped with the reason, in the order they are applied. The
  home directory is shortened to `~`, and an item is annotated with where it came from only
  when that is not the profile: `(--policy-file)`, `(command line)`, `(scan)`,
  `(hide-mounts)`, or the secret it hides;
- every scan hit left visible, and every scan root, `hide-mounts` `under`, or `path-prepend`
  entry skipped, each with the reason;
- how the environment differs from the host's: the variables unset, the variables set with
  their values (a `PATH` that only grew in front shows the part added), and the names of the
  secrets. Variables that are as on the host are counted, not listed;
- the resolved command.

`--print-plan=full` is the whole plan, for comparing two runs or for checking what reaches
`bwrap`. In place of the summary's mount list and environment changes it prints the merged
policy with the layer each entry came from, every mount item with its real path and its
origin, the final environment in full, and the `bwrap` argument list.

`--print-plan=json` is for LLM agents and tools, not for reading: the same content as the full
form, plus the summary's environment changes, as a single line of JSON. Pipe it to `jq` to look
at a part of it:

```sh
process-wrap --print-plan=json -- codex | jq -r '.mounts[] | "\(.directive)\t\(.path)"'
process-wrap --print-plan=json -- codex | jq '.environment_changes'
```

Unlike the text forms, its shape is a contract: the top-level keys below stay and keep their
meaning while `format_version` is the same; keys may be added.

| Key | Value |
| --- | --- |
| `format_version` | `1`. Raised when a key is removed or changes its meaning. |
| `nested` | Whether the run is nested (`PROCESS_WRAP=1`). |
| `policy_sources` | The policy files read, each `{"kind": "file", "path": ...}` or `{"kind": "built-in-default"}`. |
| `variables` | `workspace`, `worktree`, `git_common_dir`, `config_dir`; `null` where a variable has no value. |
| `home` | The home directory. |
| `policy` | The merged policy: `mounts` (each with `directive`, `path` as written, `origin`), `scan`, `hide_mounts`, `network_mode`, `env_mode`, `env_pass`, `env_set`, `env_unset`, `path_prepend`, `secrets`, `instead_of`. |
| `mounts` | The items applied, in order: `directive`, `path` (real), `kind` (`directory` or `not-directory`), `written`, `origin`. |
| `skipped_mounts` | Written items skipped: `directive`, `written`, `origin`, `reason`. |
| `left_visible` | Scan hits left visible: `link`, `reason`. |
| `skipped_paths` | Skipped scan roots, `hide-mounts` `under`s, and `path-prepend` entries: `role` (`scan-root`, `hide-mounts-under`, `path-prepend`), `written`, `reason`. |
| `environment` | The final environment; a secret's value is `null`. |
| `environment_changes` | `mode`, `kept`, `unset`, `set`, `secrets`, as in the summary. |
| `command` | `given`, `arguments`, `path`; `null` when `COMMAND` was left out. |
| `bwrap` | The path of `bwrap`. |
| `bwrap_arguments` | Each `{"kind": "literal", "value": ...}`, `{"kind": "seccomp-filter"}`, or `{"kind": "empty-file"}`. |

An `origin` is an object whose `kind` is `profile`, `built-in-default`, `policy-file`,
`command-line`, `scan`, `hide-mounts`, `secret`, or `config-secrets`, with `path` for the
two kinds that name a file and `name` for `secret`. Paths that are not valid UTF-8 are shown
with replacement characters; control characters are JSON-escaped.

In every form only the values of the variables the policy names under `secrets` are masked.
In the full and JSON forms every other variable of the final environment is printed with its
value as it is, and with `env.mode = "inherit"` that includes any host credential whose name
matches none of the `unset` patterns ([known gap 6](security.md#known-gaps)). Treat the
output of `--print-plan=full` and `--print-plan=json` as sensitive; the summary prints only
the values the policy set.

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
