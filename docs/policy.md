# Writing a policy

A policy file is TOML. Every section is optional; an empty file is a valid policy. The bundled
profile, [`examples/profile/default.toml`](../examples/profile/default.toml), is the built-in
default: it uses `mounts` (`rw`, `rw-file`, `ro`, `hide`, `scan`, `hide-mounts`),
`network.mode`, and `env.mode` and `env.unset`, with `secrets` present only as a comment, and
it does not use `rw-copy`, `env.pass`, `env.set`, `env.path-prepend`, or `git.instead-of`. The
fixed keys are the ones below; any other key is a `policy` error.

```toml
[mounts]
rw      = ["${workspace}", "${worktree}", "${git_common_dir}", "/tmp/kakoi", "~/.cache"]
rw-file = ["~/.claude.json"]
rw-copy = ["~/.gitconfig"]
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

## Paths and variables

Every path in a policy file is absolute, `~` alone, `~/...`, or starts with a variable. Relative
paths and `~user` are rejected. `~` is the real path of `HOME`. The variables are:

| Variable | Value |
| --- | --- |
| `${workspace}` | The workspace's real path: `--workspace`, or the current directory. |
| `${worktree}` | The first directory from the workspace upwards that has a `.git` (a directory or a regular file); the workspace itself when there is none. |
| `${git_common_dir}` | The shared `.git` of the worktree, verified against git's own back links. Has no value when the worktree is not under git. |
| `${config_dir}` | The configuration directory's real path. Has no value when the configuration directory does not exist. |

An item whose variable has no value, or whose path does not exist, is skipped and shown as
skipped in the plan. Nothing is mounted on a path that does not exist.

## Directives

| Directive | Target | Inside the isolation |
| --- | --- | --- |
| `rw` | a directory | readable and writable |
| `rw-file` | a non-directory (regular file, socket, FIFO) | writable in place; a replace via temporary file and `rename` fails |
| `rw-copy` | a directory or a regular file | a writable copy of the host's content; nothing written there reaches the host, and the copy is gone when the command ends |
| `ro` | a directory or a file | read-only |
| `hide` | a directory or a file | a directory becomes an empty directory whose contents vanish at exit; a file reads as empty |

Everything not named by the policy is visible read-only. `/dev` and `/proc` are the isolation's
own. A host UNIX socket that is visible read-only can be connected to; use `hide` to stop that.

All directives apply to the real path after resolving symbolic links, and items are mounted
ancestors first, so the narrower item wins: `hide = ["/tmp"]` with `rw = ["/tmp/kakoi"]`
gives an empty `/tmp` with only `/tmp/kakoi` shared with the host.

## `rw-copy`: writable inside, unchanged outside

`rw-copy` is for a file or a directory the command must be free to write, whose writing must
not outlive the run. Set beside `hide`, the difference is only what it starts with:

| Directive | Starts as | Writable inside | After the run |
| --- | --- | --- | --- |
| `ro` | the host's content | no | the host is unchanged |
| `hide` | empty | a hidden directory is, a hidden file is not | the host is unchanged |
| `rw-copy` | the host's content | yes | the host is unchanged, and what was written inside is gone |

The use it was written for is a configuration file an agent rewrites for itself:

```toml
[mounts]
rw-copy = ["~/.gitconfig", "~/.claude/settings.json"]
```

The CLI inside reads what you have, edits it, and reads back what it edited; the file on the
host is the one you left there.

A directory becomes a tmpfs of its own, filled at start-up from the real path: files with their
content and their permission bits, the execute bit included, subdirectories with theirs, empty
directories, and symbolic links reproduced as links with the same target text. A regular file
becomes a copy of its bytes bound over the host's file. Both live in memory, inside the mount
namespace of the run, and go with it.

What follows from that:

- The narrower item still wins. `rw-copy = ["~/.config/gh"]` under `rw = ["~/.config"]` gives a
  `gh` whose writing stops at the boundary while the rest of `~/.config` goes through, and
  `rw = ["~/.config/gh/state"]` under `rw-copy = ["~/.config/gh"]` puts that one directory back
  on the host.
- The copy is a copy: owner, timestamps, and hard links are not carried.
- Whether a file can be renamed or deleted depends on which of the two forms you wrote. An entry
  inside a copied **directory** is an ordinary file in the tmpfs, so it can be renamed, deleted,
  or replaced by the write-a-temporary-file-and-`rename` dance — none of which the host sees. A
  copied **regular file** is one mount point laid over the host's file, so, like `rw-file`, it
  takes writes in place but refuses `rename` over it and refuses to be deleted. If the tool
  writing that file replaces it rather than writing in place, name its parent directory instead.
- A symbolic link inside the copied tree is not followed when the copy is made, so no copy
  expands through one or meets a loop. Inside the isolation it resolves like any other path: one
  pointing out of the copy reaches whatever the policy makes of its target, read-only unless some
  item makes it writable, exactly as it would from anywhere else inside.
- A path that does not exist is skipped, as with every directive: the plan says so, and nothing
  is created on the host.
- Work written there is lost. An `rw-copy` over the workspace or the worktree still leaves the
  warning that no `rw` covers the work place, which is the warning you want.

The limits, and what a failure does:

- One item carries at most 4096 entries and 64 MiB of file content. The content is held in memory
  twice over, once for the descriptors handed to `bwrap` and once in the tmpfs, so a path that
  reaches past either limit stops the launch with `path`. Name something smaller, or use `ro` to
  show it without copying it.
- A source that cannot be read — a directory that cannot be listed, a file that cannot be opened —
  stops the launch with `path`, naming the item and the entry. Starting from less than the host
  has, without a word, is the one outcome `rw-copy` does not have.
- An entry no mount argument can recreate in a tmpfs — a socket, a FIFO, a device node — is left
  out and named in the plan with the reason, and the run goes on.
- The item itself must be a directory or a regular file. A socket or a FIFO at the path is a
  `path` diagnostic: there is no content to copy. Use `rw-file` for those.

## Layers

Up to three written layers are merged, lowest first: the profile, the `--policy-file`, and the
command line (`--rw`, `--hide`).

- Lists concatenate (`env.path-prepend` puts the upper layer first).
- Scalars (`network.mode`, `env.mode`) take the upper layer.
- Tables (`env.set`, `secrets`, `git.instead-of`) merge by key, with the upper layer winning.

There is no way to remove a lower layer's item; make another profile instead. If the same path
gets two different directives inside one layer, that is an error; across layers the upper layer
replaces the lower.

## Generated items

On top of the written layers, `kakoi` generates `hide` items:

- the entries `mounts.scan` matches under its root: a matching name is hidden unless it is a
  directory or a symbolic link that resolves to a directory, the policy files read are left
  alone, and a match that resolves to no real path is skipped;
- the mounts under `hide-mounts.under` whose file system type matches (never the work place
  itself);
- each secret file that exists;
- the configuration directory's `secrets/`.

A scan hit that is a symbolic link is hidden at its target, except when the target lies inside
an `ro` item you wrote: hiding it would empty your own read-only file, so the link is left
visible and the plan says why. If that `ro` item could itself be re-pointed from inside (its
path passes through a writable item), the launch stops with `path` instead. With a
`hide-mounts` written, the mount list must be readable, or the launch stops with `path` rather
than miss a mount.

## Network

`network.mode` is `host` (the host's network, the default) or `none`, which cuts the network
namespace and leaves only loopback. There is no per-domain allowance in 0.2. A proxy running
outside can still be reached from a `none` run: pass its UNIX socket with `rw-file` and point
the proxy's environment variable at it with `env.set`. That composition is unverified, and 0.2
does not guarantee it.

## Environment

The environment inside the isolation is assembled in this order and handed to `bwrap` as it is
(no environment variable travels through `bwrap`'s arguments):

1. start from the host environment (`inherit`) or from the `pass` variables alone (`clear`);
2. drop the `unset` patterns (`*` and `?` are wildcards);
3. add `set`;
4. add the secrets;
5. add the git rewrite;
6. put `path-prepend` in front of `PATH`;
7. set `KAKOI=1`.

## Secrets

Each `secrets` entry names an environment variable and a file. The variable is removed from the
environment whatever its origin, and set again from the file's content (without one trailing
newline, LF or CR LF) when the file exists; the file is then hidden inside the isolation.

- A missing file only warns.
- An empty file, one that cannot be read, one containing a NUL byte, or a value over 64 KiB is
  a `secret` error.

Keep secret files in the configuration directory's `secrets/`: that directory is always hidden,
so a secret the policy does not name cannot be read from inside either. Secret values never
appear in the plan, in warnings, or in diagnostics.

## git

`git.instead-of` maps a URL prefix to its replacement and becomes `GIT_CONFIG_KEY_n` /
`GIT_CONFIG_VALUE_n` / `GIT_CONFIG_COUNT` pairs numbered after the ones the environment already
has, so `git config --list` inside shows `url.<replacement>.insteadof=<prefix>` next to the
host's own entries.

## The work place, `/tmp`, and `/tmp/kakoi`

If no `rw` covers the workspace or the worktree, a warning is printed and the run continues. A
worktree or workspace at `/`, at the home directory, or at an ancestor of it is refused, and so
is an `rw` or `rw-file` on any of those: nothing can make the whole home writable. Starting from
a directory that ends up hidden is refused too, because `bwrap` could not change into it.

`/tmp` is hidden by the bundled profile because the host's X11 and ssh-agent sockets live there
under random names and cannot be hidden one by one. `/tmp/kakoi` is the one directory
shared with the host. You, or your shim, create it; `kakoi` never does. When it does not
exist the item is skipped and `/tmp` stays empty inside.

## Paths that are refused

A policy file, the configuration directory, a secret file, or a `path-prepend` entry must not
be inside an `rw` or `rw-file` item, and resolving its path must not pass through a symbolic
link or a directory inside one. An `rw-copy` item is not such a place: what is written there
never reaches the host, so neither a policy file's content nor a name on the way to one can be
changed from inside, and the next launch reads what this one read. Keeping a profile inside an
`rw-copy` area is allowed. A configuration directory or a secret file that is not there
yet is held to the same rule: the missing name itself carries nothing to protect, but its
deepest existing ancestor is checked, because what is missing can be created from inside the
isolation and read on the next launch.

This can look wrong at first: mounting the file read-only would seem to suffice. It does not.
The file itself cannot be moved, but its ancestor directory can be renamed from inside the
isolation, and a different file put at the same path is what the next launch reads (measured
with `bwrap` 0.9.0).

The same reasoning applies to written `rw`, `rw-file`, and `hide` items and to an explicit
`--workspace`: when their resolution passes through a writable item, the target must lie inside
an item that cannot be redirected from inside, or the launch stops with a `path` diagnostic
naming the path, the `rw` item it passed through, and the reason. Three rules refine this:

- A written `ro` item is not held to it. Re-pointing or deleting an `ro` link only moves a
  read-only place or lifts it; the one thing it could newly show, landing on a `hide`, is caught
  separately. So a dotfiles link written as `ro` passes.
- A written `rw-copy` item is not held to it either, for the same reason: it writes nowhere on
  the host, so re-pointing it only moves a copy, and the one thing a moved copy could newly
  show — landing on a `hide` — is again caught separately.
- A written `hide`, a scan `root`, or a `hide-mounts` `under` whose path follows a symbolic link
  inside an `rw` item is refused wherever it lands, even on the mount point of another `rw`
  item, from the first launch: deleting that link from inside makes the next launch skip the
  item, and what it hid shows through, or skip the origin, and nothing under it is hidden.
  Write the link's target, the real path, instead; a `hide` written as a real path inside an
  `rw` item is fine (the mount point cannot be renamed). The diagnostic names the path, the
  link followed, and the reason.
- A scan `root` or a `hide-mounts` `under` whose path passes through a writable item without
  following a link inside one must be the mount point of an `rw` item itself, not a directory
  below it: the root is not mounted, so a subdirectory can be renamed from inside and the next
  launch scans an empty tree and hides nothing.

In short: the path of a link you placed inside an `rw` area stops the launch when written as
`hide`, as a scan `root`, or as a `hide-mounts` `under`, wherever it lands, and when written as
`rw` or `rw-file` unless the link's target lies inside an item that cannot be redirected from
inside. Write the link's real target instead. The same link written as `ro` passes, but what
that `ro` protects is only as much as [known gap 15](security.md#known-gaps) says.

Landing inside something hidden is refused too, whatever the item was written as: an `rw`,
`rw-file`, `rw-copy`, or `ro` item whose path passes through a writable item and lands on or
inside a `hide` (or, for `rw` and `rw-file`, on or inside an `ro` or an `rw-copy`), including
the `hide` items `kakoi` generates for `secrets/` and for hidden mounts, would be mounted after
the wider item and show what it hid, or, over an `rw-copy`, let writing through to the host
that the policy meant to keep inside. And no item may land on `/`, `/dev`, or `/proc` or inside the latter
two: the isolation mounts those itself, and an item there would cover its view (an `ro` over
`/proc` shows the host's processes).

### Dotfiles

The case that meets this most often is dotfiles: the configuration directory's real location is
inside the dotfiles worktree, so `rw = ["${worktree}"]` would put the profile inside a writable
area. Keeping it there is fine in itself. `kakoi init` follows a link at the
configuration directory and writes at its target, which is how the directory comes to live in
dotfiles at all. What this check refuses is having that target inside a writable item of the
same launch. Write the profile for that repository like this instead, and point `--workspace`
at the subdirectory you actually work in:

```toml
[mounts]
rw = ["${workspace}", "${git_common_dir}"]
```

```sh
cd ~/dotfiles/ai
kakoi --workspace . -- codex
```

Two more rules follow from the same check:

- When you name a subdirectory of an `rw` worktree with `--workspace`, start `kakoi`
  from that directory. A `--workspace` that is the current directory is exempt from the check,
  since a process already there cannot be moved by a link swap.
- A `--workspace` whose path goes through a symbolic link, given from somewhere else, is
  refused; write the real path, or start from there. A harmless alias is caught too: with
  `rw = ["${worktree}"]` alone, `--workspace ~/proj` given from elsewhere while `~/proj` is a
  link to `~/data/proj` is refused, because the only writable item is derived from the workspace
  itself and cannot vouch for it. Give the real path, or start from inside it. An alias landing
  inside an `rw` item written by its real path, such as `rw = ["~/work"]` with
  `~/work -> ~/data/work` and `--workspace ~/work/proj`, passes.
