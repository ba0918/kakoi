# process-wrap

`process-wrap` runs a command inside a [bubblewrap](https://github.com/containers/bubblewrap)
(`bwrap`) mount namespace shaped by a layered policy, and returns the command's exit code
unchanged. It is a command-line tool for Linux on x86_64, including WSL2, and needs `bwrap` 0.9.0
or later.

The isolation has four dimensions: the file system (what is visible and what is writable), the
network (shared with the host or cut), the environment (what is inherited, dropped, and added), and
credentials (files hidden and secrets injected). The process ID, IPC, UTS, cgroup, and user
namespaces are always unshared. The boundary is assembled once at start-up and does not change
afterwards; `process-wrap` never rewrites the command's arguments, and the command sees the name
it was given as its `argv[0]` (`sh`, not `/usr/bin/sh`), inside the isolation and when nested.

The typical use is running an agent CLI (codex, opencode, Claude Code) against one repository
without giving it the rest of the home directory, the host's credentials, or the sockets that
carry them.

## Install

`process-wrap` is built from source with a Rust toolchain (1.85 or later):

```sh
cargo install --path .
```

`bwrap` must be on `PATH`; on Debian and Ubuntu it is the `bubblewrap` package.

That is the whole installation. With no profile written anywhere, `process-wrap -- COMMAND`
starts on the built-in default: the bundled [`examples/profile/default.toml`](examples/profile/default.toml),
compiled into the binary. Where the plan would name the profile file it then says
`process-wrap init` instead. Two things are empty on a machine you have just installed on:
`/tmp` is replaced by an empty directory and the shared `/tmp/process-wrap` does not exist yet,
so nothing passes through `/tmp` until you or your shim creates it; and `~/.config/gh` is hidden,
so a `gh` inside the isolation is not authenticated.

The configuration directory is `$XDG_CONFIG_HOME/process-wrap` when that variable holds an
absolute path and `~/.config/process-wrap` otherwise. The built-in default is used whenever
`profile/default.toml` is simply not there — including while `XDG_CONFIG_HOME` points into
dotfiles you have not cloned yet on a new machine. Anything else in the way, such as a broken
symbolic link or a regular file where a directory belongs, stops the launch with a `policy`
diagnostic instead, so a profile that broke is never quietly replaced by the wider default. A
profile named with `--profile NAME` never falls back either.

Then, as you need them:

- **Write the boundary out and edit it.** `process-wrap init` writes the built-in default to
  `<configuration directory>/profile/default.toml`, creates `profile/` and `secrets/` (mode
  0700) and any missing ancestor beside it, and prints the path of the file it wrote:

  ```sh
  $EDITOR "$(process-wrap init)"
  ```

  It refuses to replace anything already at that name and has no `--force`: remove the file
  first if you want it back. `process-wrap init NAME` writes `profile/NAME.toml`, which
  `--profile NAME` then selects.

- **Read what the default gives you.** It is written for WSL2: it hides the Windows drives under
  `/mnt`, `/run/WSL`, `/tmp`, `/run/user`, and the usual credential directories, opens the
  workspace and the worktree for writing, and drops the credential-shaped environment variables.
  Read the file `init` wrote, or run `process-wrap --print-plan -- true` to see what it makes of
  the machine you are on.

- **Pass a GitHub token.** Uncomment the `secrets` line in the profile `init` wrote and put the
  token in `<configuration directory>/secrets/gh-token`, one line. `secrets/` is always hidden
  inside the isolation, so the file cannot be read from in there; the value arrives as the
  environment variable named on the left of the line.

- **Wrap codex.** Copy [`examples/shim/codex`](examples/shim/codex) to a directory that comes
  before the real codex on your `PATH` and make it executable. It classifies the invocation,
  inserts codex's own sandbox-bypass flag into the forms that run the agent, copies `--cd` to
  `--workspace` and `--add-dir` to `--rw`, creates `/tmp/process-wrap`, and executes
  `process-wrap`. `PROCESS_WRAP_SHIM_OFF=1` runs the real codex instead, under its own sandbox.
  It is a template, not part of the product, so check it yourself after a codex upgrade: that
  every subcommand of `codex --help` is in one of its two lists; that running an isolated form
  with `--print-plan` shows the `--workspace` and `--rw` it copied; and that with
  `PROCESS_WRAP_SHIM_OFF=1` no `process-wrap` is started. Write your own shim for another CLI
  from this one.

- **Let an agent fit the profile to this machine.** [`skills/process-wrap-setup`](skills/process-wrap-setup)
  is an Agent Skill that proposes profile entries for the agent CLIs you have installed, a place
  for the shim, and `path-prepend` entries for replacement commands. Install it with your CLI's
  own means, such as `gh skill install ba0918/process-wrap process-wrap-setup`. Run it outside
  the isolation — before the shim is on `PATH`, with `PROCESS_WRAP_SHIM_OFF=1`, or from a CLI
  not started through `process-wrap` — because the configuration directory may not sit inside a
  writable mount item and an isolated agent therefore cannot edit its own profile. The agent is
  not isolated while it runs, so run your CLI in a mode that asks before writing, and check the
  skill for yourself: that each of its "What to keep to" items is written there as an
  instruction, and that one trial shows you a diff and asks for approval before the first write.

## Usage

```
process-wrap [OPTIONS] -- COMMAND [ARGS]...
process-wrap [OPTIONS] --print-plan [-- COMMAND [ARGS]...]
process-wrap init [NAME]
process-wrap --version
process-wrap --help
```

| Option | Meaning |
| --- | --- |
| `--profile NAME` | The profile for the global scope: `$XDG_CONFIG_HOME/process-wrap/profile/NAME.toml` (or `~/.config/process-wrap/profile/NAME.toml`). Defaults to `default`. |
| `--policy-file PATH` | A policy file for the process scope, merged on top of the profile. |
| `--workspace PATH` | The workspace. Defaults to the current directory. |
| `--rw PATH` | An `rw` directive on the command-line layer. Repeatable. |
| `--hide PATH` | A `hide` directive on the command-line layer. Repeatable. |
| `--print-plan` | Print the plan and exit without running the command. |
| `--version`, `--help` | Print the version or the usage. Each is used alone. |
| `init [NAME]` | Write the built-in default to `profile/NAME.toml` (`default` when `NAME` is left out), print its path, and exit. Used alone; see [Install](#install). |

Everything after `--` is the command and its arguments, passed through unchanged. Relative paths
given on the command line are taken from the current directory; `~` and variables are not
expanded there. Options that take a value accept `--opt VALUE` and `--opt=VALUE`; an empty value,
or a value starting with `-` in the separated form, is a usage error. Options other than `--rw`
and `--hide` can be given once.

A typical launch:

```sh
cd ~/work/project
process-wrap -- codex
```

To see what would happen without running anything:

```sh
process-wrap --print-plan -- codex
```

The plan shows the merged policy, the policy files read, the four variables, every mount item
(applied, or skipped with the reason), every scan hit left visible and every scan root,
`hide-mounts` `under`, or `path-prepend` entry skipped (each with the reason), the final
environment with secret values masked, the resolved command, and the `bwrap` argument list.

Only the values of the variables the policy names under `secrets` are masked. Every other
variable of the final environment is printed with its value as it is, and with
`env.mode = "inherit"` that includes any host credential whose name matches none of the `unset`
patterns (known gap 6 below). Treat the output of `--print-plan` as sensitive; see
[Environment, secrets, git](#environment-secrets-git) for what is masked.

### Exit codes and diagnostics

A failure of `process-wrap` itself is one line on standard error of the form
`process-wrap: <kind>: <description>`, and the exit code is 125, except that a command that cannot
be found exits 127 and, in a nested run, a command that was found but cannot be executed (a script
whose interpreter does not exist, a file of a format the kernel cannot run) exits 126. The kinds
are `usage`, `policy`, `path`, `secret`, `env`, `bwrap`, `command not found`, and
`command not executable`. Warnings are one line each starting with `process-wrap: warning: ` and
do not stop the run.

When the command runs, its exit code is returned as it is; a command killed by signal `s` yields
128 + `s`. `process-wrap` executes `bwrap` in place rather than waiting for it as a child, so a
failure of `bwrap` itself (a mount that cannot be made, an `exec` that fails) shows as `bwrap`'s
own output and exit code. Only in a nested run, where `process-wrap` executes the command itself,
does a failed `exec` become the `command not executable` diagnostic above.

Before making the file descriptors it hands to `bwrap` (one per hidden file, plus the seccomp
filter), `process-wrap` raises its soft limit on open files to the hard limit, always, so that a
scan hiding thousands of files starts under the usual limit of 1024 and the same input gives the
same result. The command inherits the raised limit. A nested run makes no descriptors and leaves
the limit alone.

## Writing a policy

A policy file is TOML. Every section is optional; an empty file is a valid policy. The bundled
profile shows all of it in use. The fixed keys are the ones below; any other key is a `policy`
error.

```toml
[mounts]
rw      = ["${workspace}", "${worktree}", "${git_common_dir}", "/tmp/process-wrap", "~/.cache"]
rw-file = ["~/.claude.json"]
ro      = ["~/.codex/AGENTS.md"]
hide    = ["/tmp", "/run/user", "~/.ssh", "~/.aws", "/run/WSL"]

[[mounts.scan]]
root    = "${worktree}"                # required
names   = [".env", ".env.*"]           # required, not empty
exclude = ["*.example", "*.sample"]    # optional
prune   = [".git", "node_modules"]     # optional

[[mounts.hide-mounts]]
under  = "/mnt"                        # required
fstype = ["9p", "drvfs"]               # required, not empty

[network]
mode = "host"            # "host" | "none"; default "host"

[env]
mode         = "inherit" # "inherit" | "clear"; default "inherit"
pass         = []        # only meaningful when the merged mode is "clear"
set          = { }
unset        = ["SSH_AUTH_SOCK", "*_TOKEN"]
path-prepend = []

[secrets]
GH_TOKEN = "${config_dir}/secrets/gh-token"

[git.instead-of]
"git@github.com:" = "https://github.com/"
```

### Paths and variables

Every path in a policy file is absolute, `~` alone, `~/...`, or starts with a variable. Relative
paths and `~user` are rejected. `~` is the real path of `HOME`. The variables are:

| Variable | Value |
| --- | --- |
| `${workspace}` | The workspace's real path: `--workspace`, or the current directory. |
| `${worktree}` | The first directory from the workspace upwards that has a `.git` (a directory or a regular file); the workspace itself when there is none. |
| `${git_common_dir}` | The shared `.git` of the worktree, verified against git's own back links. Has no value when the worktree is not under git. |
| `${config_dir}` | The configuration directory's real path. |

An item whose variable has no value, or whose path does not exist, is skipped and shown as
skipped in the plan. Nothing is mounted on a path that does not exist.

### Directives

| Directive | Target | Inside the isolation |
| --- | --- | --- |
| `rw` | a directory | readable and writable |
| `rw-file` | a non-directory (regular file, socket, FIFO) | writable in place; a replace via temporary file and `rename` fails |
| `ro` | a directory or a file | read-only |
| `hide` | a directory or a file | a directory becomes an empty directory whose contents vanish at exit; a file reads as empty |

Everything not named by the policy is visible read-only. `/dev` and `/proc` are the isolation's
own. A host UNIX socket that is visible read-only can be connected to; use `hide` to stop that.
All directives apply to the real path after resolving symbolic links, and items are mounted
ancestors first, so the narrower item wins: `hide = ["/tmp"]` with `rw = ["/tmp/process-wrap"]`
gives an empty `/tmp` with only `/tmp/process-wrap` shared with the host.

### Layers

Up to three written layers are merged, lowest first: the profile, the `--policy-file`, and the
command line (`--rw`, `--hide`). Lists concatenate (`env.path-prepend` puts the upper layer
first), scalars (`network.mode`, `env.mode`) take the upper layer, and tables (`env.set`,
`secrets`, `git.instead-of`) merge by key with the upper layer winning. There is no way to remove
a lower layer's item; make another profile instead. If the same path gets two different
directives inside one layer, that is an error; across layers the upper layer replaces the lower.

On top of the written layers, `process-wrap` generates `hide` items: the files found by
`mounts.scan`, the mounts under `hide-mounts.under` whose file system type matches (never the
work place itself), each secret file that exists, and the configuration directory's `secrets/`.
A scan hit that is a symbolic link is hidden at its target, except when the target lies inside an
`ro` item you wrote: hiding it would empty your own read-only file, so the link is left visible
and the plan says why; if that `ro` item could itself be re-pointed from inside (its path passes
through a writable item), the launch stops with `path` instead. With a `hide-mounts` written, the
mount list must be readable, or the launch stops with `path` rather than miss a mount.

### Environment, secrets, git

The environment inside the isolation is assembled in this order and handed to `bwrap` as it is
(no environment variable travels through `bwrap`'s arguments): start from the host environment
(`inherit`) or from the `pass` variables alone (`clear`); drop the `unset` patterns (`*` and `?`
are wildcards); add `set`; add the secrets; add the git rewrite; put `path-prepend` in front of
`PATH`; set `PROCESS_WRAP=1`.

Each `secrets` entry names an environment variable and a file. The variable is removed from the
environment whatever its origin, and set again from the file's content (without one trailing
newline, LF or CR LF) when the file exists; the file is then hidden inside the isolation. A missing file only
warns. An empty file, one that cannot be read, one containing a NUL byte, or a value over 64 KiB
is a `secret` error. Keep secret files in the configuration directory's `secrets/`: that directory
is always hidden, so a secret the policy does not name cannot be read from inside either. Secret
values never appear in the plan, in warnings, or in diagnostics.

`git.instead-of` maps a URL prefix to its replacement and becomes `GIT_CONFIG_KEY_n` /
`GIT_CONFIG_VALUE_n` / `GIT_CONFIG_COUNT` pairs numbered after the ones the environment already
has, so `git config --list` inside shows `url.<replacement>.insteadof=<prefix>` next to the
host's own entries.

### The work place, `/tmp`, and `/tmp/process-wrap`

If no `rw` covers the workspace or the worktree, a warning is printed and the run continues. A
worktree or workspace at `/`, at the home directory, or at an ancestor of it is refused, and so is
an `rw` or `rw-file` on any of those: nothing can make the whole home writable. Starting from a
directory that ends up hidden is refused too, because `bwrap` could not change into it.

`/tmp` is hidden by the bundled profile because the host's X11 and ssh-agent sockets live there
under random names and cannot be hidden one by one. `/tmp/process-wrap` is the one directory
shared with the host. You, or your shim, create it; `process-wrap` never does. When it does not
exist the item is skipped and `/tmp` stays empty inside.

### Why a policy file inside a writable area is refused

A policy file, the configuration directory, a secret file, or a `path-prepend` entry must not be
inside an `rw` or `rw-file` item, and resolving its path must not pass through a symbolic link or
a directory inside one. A configuration directory or a secret file that is not there yet is held
to the same rule: the missing name itself carries nothing to protect, but its deepest existing
ancestor is checked, because what is missing can be created from inside the isolation and read on
the next launch. This can look wrong at first: mounting the file read-only would seem to
suffice. It does not. The file itself cannot be moved, but its ancestor directory can be renamed
from inside the isolation, and a different file put at the same path is what the next launch
reads (measured with `bwrap` 0.9.0). The same reasoning applies to written `rw`, `rw-file`, and
`hide` items and to an explicit `--workspace`: when their resolution passes through a writable
item, the target must lie inside an item that cannot be redirected from inside, or the launch
stops with a `path` diagnostic naming the path, the `rw` item it passed through, and the reason.
Three rules refine this:

- A written `ro` item is not held to it. Re-pointing or deleting an `ro` link only moves a
  read-only place or lifts it; the one thing it could newly show, landing on a `hide`, is caught
  separately. So a dotfiles link written as `ro` passes.
- A written `hide`, a scan `root`, or a `hide-mounts` `under` whose path follows a symbolic link
  inside an `rw` item is refused wherever it lands, even on the mount point of another `rw` item,
  from the first launch: deleting that link from inside makes the next launch skip the item, and
  what it hid shows through, or skip the origin, and nothing under it is hidden. Write the link's
  target, the real path, instead; a `hide` written as a real path inside an `rw` item is fine (the
  mount point cannot be renamed). The diagnostic names the path, the link followed, and the
  reason.
- A scan `root` or a `hide-mounts` `under` whose path passes through a writable item without
  following a link inside one must be the mount point of an `rw` item itself, not a directory
  below it: the root is not mounted, so a subdirectory can be renamed from inside and the next
  launch scans an empty tree and hides nothing.

In short: the path of a link you placed inside an `rw` area stops the launch when written as
`hide`, as a scan `root`, or as a `hide-mounts` `under`, wherever it lands, and when written as
`rw` or `rw-file` unless the link's target lies inside an item that cannot be redirected from
inside; write the link's real target instead. The same link written as `ro` passes, but what that
`ro` protects is only as much as known gap 15 below says.

Landing inside something hidden is refused too, whatever the item was written as: an `rw`,
`rw-file`, or `ro` item whose path passes through a writable item and lands on or inside a
`hide` (or, for `rw` and `rw-file`, on or inside an `ro`), including the `hide` items
`process-wrap` generates for `secrets/` and for hidden mounts, would be mounted after the wider
item and show what it hid. And no item may land on `/`, `/dev`, or `/proc` or inside the latter
two: the isolation mounts those itself, and an item there would cover its view (an `ro` over
`/proc` shows the host's processes).

The case that meets this most often is dotfiles: the configuration directory's real location is
inside the dotfiles worktree, so `rw = ["${worktree}"]` would put the profile inside a writable
area. Keeping it there is fine in itself — `process-wrap init` follows a link at the
configuration directory and writes at its target, which is how the directory comes to live in
dotfiles at all — and what this check refuses is having that target inside a writable item of the
same launch. Write the profile for that repository like this instead, and point `--workspace` at
the subdirectory you actually work in:

```toml
[mounts]
rw = ["${workspace}", "${git_common_dir}"]
```

```sh
cd ~/dotfiles/ai
process-wrap --workspace . -- codex
```

Two more rules follow from the same check. When you name a subdirectory of an `rw` worktree with
`--workspace`, start `process-wrap` from that directory: a `--workspace` that is the current
directory is exempt from the check, since a process already there cannot be moved by a link swap.
And a `--workspace` whose path goes through a symbolic link, given from somewhere else, is refused;
write the real path, or start from there. A harmless alias is caught too: with
`rw = ["${worktree}"]` alone, `--workspace ~/proj` given from elsewhere while `~/proj` is a link to
`~/data/proj` is refused, because the only writable item is derived from the workspace itself and
cannot vouch for it. Give the real path, or start from inside it. (An alias landing inside an `rw`
item written by its real path, such as `rw = ["~/work"]` with `~/work -> ~/data/work` and
`--workspace ~/work/proj`, passes.)

## Known gaps

1. A command inside a hidden directory is still found by the `PATH` search, which runs on the
   host file system; the launch then fails at `bwrap`'s `exec` with `bwrap`'s own output and
   exit code.
2. A file that appears after start-up is not hidden. `bwrap` would create a mount point for a
   missing path and leave an empty file on the host, so nothing is mounted on a path that does
   not exist.
3. A file hidden by the scan that git tracks shows up inside as a change that emptied it.
4. `hide` acts on the real path it names. A bind mount or a hard link that reaches the same
   content by another path is not hidden.
5. The paths of mount items and secret files are `bwrap` arguments and visible in the process
   list; the secret values are in the isolated process's environment and readable from the host
   through `/proc`. The host is the trusted side.
6. With `env.mode = "inherit"`, a credential in the host environment whose name matches none of
   the `unset` patterns enters the isolation.
7. Only `TIOCSTI` is blocked by the seccomp filter. `TIOCLINUX`, injection through terminal
   responses, and input synthesis through a display server's socket are not; the bundled profile
   cuts the socket paths with `hide` and the variables with `unset`.
8. Nesting is detected only through `PROCESS_WRAP=1`. Clearing the environment inside the
   isolation and starting `process-wrap` again attempts a second isolation (no wider than the
   first; a policy with secrets fails there because the outer isolation emptied the files).
   Setting `PROCESS_WRAP=1` on the host runs the command without isolation, with the nesting
   warning on standard error.
9. `process-wrap` trusts the environment it starts in: `HOME`, `XDG_CONFIG_HOME`, `PATH`,
   `PROCESS_WRAP`, and the current directory. That includes the current directory: `cd` into a
   path that passes through an `rw` area, after a link there was swapped from inside, and the
   link's new target becomes the work place.
10. An `rw` area is a place for anything the user later runs on the host. `.git/hooks` and
    `.git/config` are read by the user's own `git`; the isolation cannot prevent that, only a
    look at the diff can.
11. 32-bit and x32 binaries do not run inside: the seccomp filter ends any process that makes a
    system call for another architecture or with the x32 bit set.
12. Deleting the worktree's `.git` from inside can make the next launch derive the worktree from
    an ancestor repository. The home directory and its ancestors are refused as a worktree;
    ancestors below that are not.
13. A main worktree made with `git init --separate-git-dir` (`.git` is a regular file whose
    target has neither `commondir` nor `core.worktree`) matches neither of the two verified
    layouts and stops with `path`.
14. The check on redirected items is made against the writable items of the current launch. An
    item written literally below the worktree (`rw = ["${worktree}", "~/work/a/b"]`) can have
    `~/work/a` swapped for a link while the worktree is `~/work`, and a later launch with a
    different worktree does not see that and applies `rw` to the link's target. The same holds
    when the item's own path is swapped for a link (`~/work/a/b` replaced, then
    `--workspace ~/work/a/b/inner` given from elsewhere on the next launch). Launched from the
    same worktree, both stop. Closing this would need remembering the previous launch, which
    `process-wrap` does not do; write subdirectories of the worktree with variables.
15. `ro` and `hide` items inside an `rw` area protect less than they seem to. An `ro` written as
    a link protects only the link's target: from inside, the link can be deleted and a regular
    file of the same name put in its place, and whatever reads that path in the same launch sees
    the new content (an editor that saves through a temporary file and `rename` replaces the link
    too). When the target is outside every writable item, that place was read-only already and
    the `ro` item added nothing. On the next launch, a re-pointed link makes some other place
    read-only and a deleted one lifts the read-only elsewhere (anything newly visible is stopped
    by the exposing-pair check; a `hide` written as a link is stopped from the first launch).
    Even without links, renaming an ancestor directory and placing another file at the same path
    changes what the next launch reads. What `ro` guarantees is that the content the agent reads
    is not changed under it, not that the agent cannot be steered into reading something else.

## Not in 0.1

No cgroup limits, no per-domain network allowance, no removal operator in the merge, no automatic
merge of `default.toml` under another profile, no policy files found from the current directory,
no `--new-session`, no double isolation when nested, no aarch64, no protection of `.git/hooks`
and `.git/config`, no shims for claude or opencode (write your own from
[`examples/shim/codex`](examples/shim/codex)), no `init --force` (remove the file first), and no
installer for the setup skill (use your agent CLI's own means, such as
`gh skill install ba0918/process-wrap process-wrap-setup`).

## Specification

The complete behaviour is specified in [`docs/spec/process-wrap.md`](docs/spec/process-wrap.md)
(Japanese); the glossary is [`CONTEXT.md`](CONTEXT.md). The version lives in `Cargo.toml` only.
