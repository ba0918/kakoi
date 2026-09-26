# Writing a policy

A policy file is TOML. Every section is optional; an empty file is a valid policy. The bundled
profile, [`examples/profile/default.toml`](../examples/profile/default.toml), is the built-in
default: it uses `mounts` (`rw`, `rw-file`, `ro`, `hide`, `scan`, `hide-mounts`),
`network.mode`, and `env.mode` and `env.unset`, with `secrets` present only as a comment, and
it does not use `rw-copy`, `env.pass`, `env.set`, `env.path-prepend`, or `git.instead-of`, and
carries a command guard for `git` only as a comment. The fixed keys are the ones below and the
keys of a [command guard](#command-guards); any other key is a `policy` error.

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
mode = "host"            # "host" | "none" | "filtered"; default "host"

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

[[commands.guard]]
program = "git"
reason  = "pushing is left to the person; ask them to push"
deny    = [["push"]]
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

- Lists concatenate (`env.path-prepend` puts the upper layer first). `commands.guard` is a list:
  an upper layer adds rules and cannot remove a lower layer's.
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

`network.mode` is one of:

- `host`, the default: the host's network, unchanged;
- `none`: the network namespace is cut and only loopback is left. A proxy running outside can
  still be reached from a `none` run: pass its UNIX socket with `rw-file` and point the proxy's
  environment variable at it with `env.set`. That composition is unverified, and `kakoi` does
  not guarantee it;
- `filtered`: new connections leave only when they match an allow rule, and ports are
  published to the host only when you name them. It needs `pasta` (from the `passt` package)
  and `nft` (nftables) on `PATH`; `host` and `none` need neither.

`filtered` has to be written. Writing allow rules or publications never switches the mode by
itself: with `mode` written nowhere and such settings present, the launch stops. With `host` or
`none` chosen, leftover `filtered` settings are still checked for their form, then ignored with a
warning.

### Allow rules

```toml
[network]
mode = "filtered"

[[network.allow]]
destination = { dns = "api.example.com" }   # or { ip = "..." }, { cidr = "..." }
protocol = "tcp"                            # "tcp" | "udp"
ports = ["443"]
```

Every rule has `destination`, `protocol`, and `ports`. A new connection goes out only when its
destination, protocol, and port match a rule; the reply traffic of an allowed connection comes
back. Loopback inside the isolation is always open on every port, and it is not the host's
loopback.

- **`ip` and `cidr`** allow an address or a network, IPv4 or IPv6.
- **`dns`** allows the addresses the name resolves to. `example.com` is that name alone;
  `*.example.com` is every name below it, at any depth, and not `example.com` itself. Case and
  one trailing dot do not matter, and internationalised names are accepted and converted to
  ASCII. The application resolves through a resolver `kakoi` runs; an address it answers is
  allowed until the answer's time to live runs out (1 second for a time to live of 0, set with
  `network.dns-zero-ttl-grace-milliseconds`). A connection already established stays up after
  that, as does a UDP exchange that keeps going (`network.udp-idle-timeout-seconds`, default
  120). Loopback, private, and other special addresses in an answer are not allowed by a
  `dns` rule alone; allow them with `ip` or `cidr`.
  The resolver answers the same question again from what it last received while that
  answer lives, and a question that just failed fails again without asking for
  `network.dns-failure-cache-seconds` (default 5).
- **`ports`** is a list of strings: `"443"`, a range `"8000-8010"`, or `"*"` for every port
  alone. Numbers are 1 to 65535, without leading zeros, signs, or spaces; service names and
  bare integers are refused.

To reach a service on the host's own loopback, allow `host-loopback` and connect to
`host-v4.kakoi.internal` (the host's `127.0.0.1`) or `host-v6.kakoi.internal` (`::1`). Inside,
`localhost` stays the isolation's own.

```toml
[[network.allow]]
destination = { host-loopback = "ipv4" }    # "ipv4" | "ipv6"
protocol = "tcp"
ports = ["8080"]
```

IPv6 link-local destinations (`host-interface`) are not available yet: a rule with one is
refused before the launch.

### DNS upstream

With no `[[network.dns-upstream]]`, the resolver asks the `nameserver`s of the host's
`/etc/resolv.conf`, in order, and follows the file when it changes. (A file naming only
systemd-resolved's stub `127.0.0.53` is read as `127.0.0.54`.) To name the upstream instead:

```toml
[[network.dns-upstream]]
transport = "tls"               # "plain" | "tls"
ip = "192.0.2.53"
port = 853
tls-name = "resolver.example.com"   # required for "tls", refused for "plain"
```

Several upstreams are tried in order; plain and TLS are not mixed. TLS verifies the server with
the host's CA certificates. The waits are `network.dns-server-timeout-seconds` (default 2) for each
upstream and `network.dns-resolution-timeout-seconds` (default 10) for the whole resolution.
Following the host, each `nameserver` gets the per-upstream wait, except systemd-resolved's
`127.0.0.54` used alone: it tries the host's servers itself and is waited for up to the whole
resolution.

### Publishing a port

```toml
[[network.publish]]
mode = "fixed"
protocol = "tcp"          # "tcp" | "udp"
port = 8000               # inside the isolation
host-port = 18000         # on the host
# target-family = "ipv4"  # "ipv4" | "ipv6"; default "ipv4"
# host-family = "ipv4"    # must equal target-family
```

The host listens on `127.0.0.1` (or `::1`) only, and forwards to the same family's loopback
inside. The port is taken before the command starts and held until the isolation ends,
whether or not anything listens inside yet. When it cannot be taken, the launch fails with 125
and the command does not run; `kakoi` never picks another number. Each published port is
announced on standard error when the run starts:

```
kakoi: network published: tcp 127.0.0.1:18000 -> sandbox 127.0.0.1:8000
```

### The end of a run

When the main command ends, the network and the publications are stopped first; then the
processes it left behind are asked to end and given a common grace, `process.shutdown-grace-seconds`
(default 5, 1 to 300), before they are killed. See
[Exit codes and diagnostics](cli.md#exit-codes-and-diagnostics) for what a `filtered` run
returns.

```toml
[process]
shutdown-grace-seconds = 5
```

### Layers

`network.allow`, `network.dns-upstream`, and `network.publish` add up across the layers; an
empty list removes nothing. Two publications that give the same endpoint different
counterparts are an error. The numeric settings take the upper layer.

The other numeric settings, all under `[network]`, bound the resolver's work and are seldom
needed: `dns-max-cname-hops` (16), `dns-max-upstream-queries` (64),
`dns-max-concurrent-resolutions` (256), `dns-max-waiters-per-resolution` (64),
`dns-failure-cache-seconds` (5), and `recovery-attempt-timeout-seconds` (10).

## Environment

The environment inside the isolation is assembled in this order and handed to `bwrap` as it is
(no environment variable travels through `bwrap`'s arguments):

1. start from the host environment (`inherit`) or from the `pass` variables alone (`clear`);
2. drop the `unset` patterns (`*` and `?` are wildcards);
3. add `set`;
4. add the secrets;
5. add the git rewrite;
6. put `path-prepend` in front of `PATH`;
7. put the directory of the [command guards](#command-guards) first on `PATH`, when a guard is
   placed and there is a `PATH`;
8. set `KAKOI=1`.

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

## Command guards

A command guard stops one way of using a program the isolated process starts, such as
`git push`, while leaving the rest of the program usable. It is a guardrail against mistakes,
not a boundary: a process that means to get around it can (see
[Command guards are not a boundary](security.md#command-guards-are-not-a-boundary)). To stop
something for certain, use a token with narrower permissions or the `filtered` network's allow
rules; to keep a program from being used at all, `hide` it, and when what you protect is a
resource (the docker socket, say) rather than the program, `hide` the resource.

```toml
[[commands.guard]]
program            = "git"                  # required: a name looked up on PATH, no "/", not "kakoi"
reason             = "pushing is left to the person; ask them to push"   # required, not empty
options-with-value = ["-C", "-c", "--config-env", "--git-dir", "--work-tree", "--namespace"]
deny               = [["push"], ["remote", ["add", "set-url"]]]
deny-option-values = { "-c" = ["/(?i)alias[.].*/"], "--config-env" = ["/(?i)alias[.].*/"] }
guard-absolute-path = false                  # optional; default false
examples.deny      = ["git push", "git -C repo push origin main", "git -c alias.p=push p"]
examples.allow     = ["git status", "git commit -m 'push fix'", "git -c color.ui=false log"]

[[commands.guard]]
program            = "git"
reason             = "commits run the hooks; fix what they report instead"
options-with-value = ["-C", "-c", "--config-env", "--git-dir", "--work-tree", "--namespace"]
for                = [["commit"]]            # optional: limits deny-flags, deny-option-values, deny-env
deny-flags         = ["--no-verify", "-n"]
deny-env           = ["HUSKY"]
examples.deny      = ["git commit --no-verify -m x", "git commit -nm x", "HUSKY=0 git commit -m x"]
examples.allow     = ["git commit -m x", "git log -n 3", "HUSKY=0 git status"]
```

A rule needs at least one of `deny`, `deny-flags`, `deny-option-values`, and `deny-env`. A list
or a word sequence written empty, an unknown key, and a `program` that is empty, contains `/`, or
is `kakoi` are `policy` errors.

How a run is matched, given the words after the program name:

- **Words.** A word in `deny` or `for` and a value in `deny-option-values` match a whole word:
  a text between two `/` (`"/pu.h/"`) is a regular expression in the syntax of Rust's `regex`
  crate, applied to the whole word; anything else must equal the word. Option names are always
  compared literally. A word that is not valid UTF-8 matches nothing.
- **`deny`.** Leading words that start with `-` are skipped first: a name listed in
  `options-with-value` skips the next word too, a word with `=` is skipped alone, and any other
  such word is taken to have no value. A `--` word is skipped and ends the skipping. The run is
  denied when the words left start with one of the `deny` sequences; each position of a
  sequence is one word or a list of alternatives, and words after the sequence do not matter.
  Options in the middle of the sequence are not skipped.
- **`deny-flags`.** Matched against every word before the first `--`, the part before the first
  `=` of a word (`--force=yes` is `--force`); a one-letter flag also matches inside a bundle
  (`-f` matches `-fq`).
- **`deny-option-values`.** Before the first `--`, the word after the option's name, or what
  follows the first `=` of `name=value`, matched against the option's list.
- **`deny-env`.** A variable whose name matches one of the patterns (`*` and `?` as in
  `env.unset`) is set in the environment the guard is started with.
- **`for`.** When present, `deny-flags`, `deny-option-values`, and `deny-env` apply only to a run
  whose leading words (after the skipping) match one of its sequences; `deny` always applies.

Within a rule the order is `deny-env`, `deny`, `deny-flags`, `deny-option-values`, and the first
match denies. A run no rule denies is not denied.

**Examples are checked when the policy is read.** Each example is split into words as a shell
would split it; leading `NAME=value` words are the example's whole environment (the host's is not
used), and the first word left must be the rule's `program`. Every `examples.deny` must be denied
by its rule and every `examples.allow` must not; an example that does not match as expected, a
broken regular expression, and a malformed rule stop the run with `policy`, `--print-plan`
included, naming the program and the example.

**The guard.** For each program with a rule, `kakoi` looks the program up on the `PATH` the
isolation gets (after `path-prepend`, leaving out the guards' own directory, which a nested
`kakoi` inherits) and places a guard of the same name in a directory of its
own, first on `PATH`. The guard is `kakoi`'s own executable, placed read-only in a tmpfs of its
own together with the rules; it is laid over every mount item. When a run is denied, the guard
prints `kakoi: guard: <program> <the words that matched>: <reason>` on standard error and exits
126 without starting the program; otherwise it executes the real program in its own place, with
the same `argv[0]`, arguments, environment, and working directory. A command given to `kakoi`
itself (`kakoi -- git push`) goes through the guard too. A program is skipped, with the reason
shown in the plan, when it is not found on that `PATH`, when it is hidden by a `hide` item, when
what is found is not a regular file or is `kakoi` itself, or when the isolation has no `PATH`;
the rule is still checked.

**`guard-absolute-path`.** By default only a start through `PATH` meets the guard, and
`/usr/bin/git push` does not. With `guard-absolute-path = true` the guard is also laid over the
real program's own path, and the program is placed again inside the guard's tmpfs. A program that
finds its resources relative to its own location (Python's standard library, for one) can break
when it is moved this way; use it for programs such as `git` that do not.

**`git.instead-of` and `GIT_CONFIG_*`.** `git.instead-of` works by putting `GIT_CONFIG_*`
variables in the environment, so a rule that denies `GIT_CONFIG_*` with `deny-env` stops every
`git` command. The two cannot be used together; the example in the bundled profile denies the
`-c` and `--config-env` values that define an alias instead.

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
