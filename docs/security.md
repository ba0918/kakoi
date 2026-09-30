# Security model

`kakoi` narrows what a process can see and touch. It is not a replacement for a
container or a VM: it shares the host's kernel, trusts the host it is started from, and sets no
resource limits. This page states the boundary, what it guarantees, and where it is known to
leak.

## The trust boundary

The host is the trusted side; the isolated process is not. `kakoi` trusts the environment
it starts in (`HOME`, `XDG_CONFIG_HOME`, `XDG_RUNTIME_DIR`, `PATH`, and the current directory),
the policy files it reads, and the `bwrap` it finds on `PATH` (and, in `filtered` mode, the
`pasta` and `nft`). Everything the host does, your
shell, your `git`, the files you open afterwards, is outside the boundary.

The isolation has four dimensions:

- **file system**: what is visible and what is writable, as the policy's mount items say;
- **network**: shared with the host, cut, or `filtered` to the destinations and ports the
  policy allows;
- **environment**: what is inherited, dropped, and added;
- **credentials**: files hidden and secrets injected.

The process ID, IPC, UTS, and user namespaces are always unshared, and a launch is refused on a
machine that cannot create a user namespace. The cgroup namespace is unshared as well, except
where the kernel does not support it. What this model covers is the non-setuid `bwrap`, run as a
user other than root. A seccomp filter
fails `ioctl(TIOCSTI)` with `EPERM` and ends any process that makes a system call for another
architecture or with the x32 bit set. The boundary is assembled once at start-up and does not
change afterwards, except that a `filtered` network follows the DNS answers for the names it
allows.

## What is guaranteed

When the launch is not refused:

- the process sees the file system the policy describes, and nothing the policy hides; under
  the `listed` [mount mode](policy.md#showing-only-what-is-listed-mountsmode), nothing but the
  base and the places the policy lists is there at all, and with the `host` network no abstract
  UNIX socket made outside can be reached;
- `rw` and `rw-file` items are the only places whose writes reach the host; an `rw-copy` item is
  writable from inside, and everything written there lives in the isolation's own tmpfs and goes
  with it;
- the secrets the policy names, the files they come from, and the configuration directory's
  `secrets/` are not readable from inside;
- a policy file, the configuration directory, a secret file, or a `path-prepend` entry that
  could be swapped from inside, so that the next launch reads something else, is refused before
  the launch (see [Paths that are refused](policy.md#paths-that-are-refused));
- keystrokes cannot be pushed into your terminal through `TIOCSTI`;
- the exit code you get is the command's own.

## What is not guaranteed

- Anything about the host. A compromised host, a swapped `bwrap`, or a `cd` through a link an
  earlier session re-pointed is outside the model.
- Resource limits: no cgroup limits on CPU or memory.
- Anything about traffic `filtered` allows: an allowed destination can carry whatever the
  process sends it.
- Kernel isolation: the process shares the host's kernel, as with any namespace-based sandbox.
- That an `rw` area stays harmless afterwards. What the process writes there (`.git/hooks`,
  `.git/config`, build scripts) is read by whatever you later run on the host.
- Complete protection of `ro` and `hide` items placed inside an `rw` area; see known gap 14.
- That a command guard stops what it names: a guard is a guardrail, not part of the boundary
  (see below). The same holds for the `listed` command mode.

## The `filtered` network

In `filtered` mode `kakoi` stays running as a supervisor instead of handing the process to
`bwrap`. The rules live in the kernel (nftables) of network namespaces the isolated process
cannot administer, and the traffic is carried by two `pasta` processes outside it. What holds:

- a new connection leaves only when its destination, protocol, and port match an allow rule;
  an address learned by DNS counts only when the answer came through `kakoi`'s own resolver,
  and only until the answer's time to live runs out;
- the host's loopback is reachable only through `host-loopback` rules, and a port is published
  to the host only when a `[[network.publish]]` names it. The host's other addresses are
  reachable only through `ip` or `cidr` rules: a `dns` rule whose name resolves to one opens
  nothing;
- when the enforcement fails while running (a `pasta` process, the supervisor, or its watchdog
  stops), all traffic in and out is blocked, the process keeps running, and `kakoi` rebuilds
  the same rules behind the block before opening again. Addresses whose DNS answers expired
  meanwhile are not restored. A kernel lease that the supervisor renews every second closes the
  path by itself when neither the supervisor nor the watchdog can act. When even the block
  cannot be confirmed, every process is killed at once and `kakoi` exits 125;
- when the main command ends, traffic and publications stop before the processes left behind
  are asked to end.

Limits of `filtered`, as it stands:

- `pasta` gives each UDP flow a host-side socket with the same port number the process sent
  from. When that number is already taken on the host, the flow cannot be made and its
  datagrams are dropped without notice; the process sees a timeout. Sending again from another
  port is a new flow. This is `pasta`'s behaviour and is accepted.
- the host's own addresses are read once, at start-up. An address the host gains later (a VPN
  coming up, say) counts as any other, and a `dns` rule whose name resolves to it opens it.
- IPv6 link-local destinations (`host-interface`) are refused, and publications are fixed:
  nothing is published because something inside started listening.

## Command guards are not a boundary

A [command guard](policy.md#command-guards) stops a way of using a program that an agent would
reach for by mistake, with a reason; it does not stop a process that means to get around it. It
does not watch:

- a start from a place where no guard is placed: an absolute path without
  `guard-absolute-path`, another name or a hard link that reaches the same program, and the real
  program found again where `guard-absolute-path` placed it;
- a start in an environment without `PATH` (a policy with `env.mode = "clear"` that passes
  none, including an inner isolation made with `--nested=isolate`): the guards' directory is put
  on `PATH` only when there is one;
- a library that does the same work without starting the program (a git implementation in
  Python, say);
- an alias defined in a configuration file (the `[alias]` of a `.git/config` writable from
  inside) or through environment variables (`GIT_CONFIG_*`);
- a shell's built-in commands;
- an abbreviated long option (`git push --forc`);
- an option in the middle of the word sequence (`git remote -v add` is not matched by
  `["remote", "add"]`);
- the guard's own content when `kakoi`'s executable lies under an item writable from inside:
  the guards, the rules, and a relocated program cannot be written through the guards'
  directory, but the guard is `kakoi`'s own executable, and writing it through that item
  changes every guard (and the `kakoi` on the host);
- a file swapped in the gap between the plan and the start under `guard-absolute-path`: the
  real program is relocated by the path resolved when the plan was made, and `bwrap` resolves
  that path again when it starts, so another isolation that can write where the real program
  lies can put a hidden file there in between, and it shows at the relocated place. This is the
  same kind of gap as item 14 of the known gaps below.

A denial is reported only by the guard's own line on standard error and exit code 126; nothing
is sent outside the isolation. To stop an operation for certain, give the process a token with
narrower permissions or limit its traffic with the `filtered` network's allow rules. To keep a
program from being used at all, `hide` it; when what needs protecting is a resource (the docker
socket, say), `hide` the resource, since the program is not the only way to reach it.

## Allowing only listed programs is not a boundary either

The `listed` [command mode](policy.md#allowing-only-listed-programs-commandsmode) stops every
program `commands.allow` does not list from starting, with Landlock, so unlike a command guard it
also stops a program nobody thought of. It is still a guardrail: a program that is allowed can do
what an unlisted one would without starting it. The dynamic linker, which `kakoi` has to allow
for anything to start, runs any readable file as a program when it is started by its absolute
path; an allowed interpreter (`python3`, `node`) can read a file and run it as code from memory;
and an executable copied into a `memfd` starts. Allowing an interpreter opens that way, so a list
that should mean something leaves them out. What keeps sockets and credentials out of reach is
the `listed` mount mode and the `filtered` network, not the list of programs. See known gaps 17
and 18.

## Known gaps

1. A file that appears after start-up is not hidden. `bwrap` would create a mount point for a
   missing path and leave an empty file on the host, so nothing is mounted on a path that does
   not exist.
2. A file hidden by the scan that git tracks shows up inside as a change that emptied it.
3. `hide` acts on the real path it names. A bind mount or a hard link that reaches the same
   content by another path is not hidden.
4. The paths of mount items and secret files are `bwrap` arguments and visible in the process
   list; the secret values are in the isolated process's environment and readable from the host
   through `/proc`. The host is the trusted side.
5. With `env.mode = "inherit"`, a credential in the host environment whose name matches none of
   the `unset` patterns enters the isolation.
6. Only `TIOCSTI` is blocked by the seccomp filter. `TIOCLINUX`, injection through terminal
   responses, and input synthesis through a display server's socket are not; the bundled profile
   cuts the socket paths with `hide` and the variables with `unset`.
7. Nesting and isolations made inside one have limits. A process inside that makes a namespace
   of its own and covers `/dev` hides the nesting mark from a `kakoi` it starts there, which
   then does not take itself to be nested; nor does a `kakoi` started inside an isolation of an
   older version, which put no mark. A third isolation, made with `--nested=isolate` inside one
   made the same way, stops without running its command when it mounts at a path where the
   second one put a file made from data (a hidden file, or `/etc/resolv.conf` in `filtered`);
   so does the second one when the outer launch could not use the shared file place. When both
   the outer and the inner policy place command guards, the inner isolation is not made either.
   A `kakoi` inside that did not take itself to be nested can make a shared file place in a
   writable place of the outer isolation, and the outer agent can then put content into the
   files that launch hides. When the outer and the inner policy make the same file
   `rw-copy`, the inner isolation cannot be made.
8. `kakoi` trusts the environment it starts in: `HOME`, `XDG_CONFIG_HOME`, `XDG_RUNTIME_DIR`,
   `PATH`, and the current directory. That includes the current directory: `cd` into a
   path that passes through an `rw` area, after a link there was swapped from inside, and the
   link's new target becomes the work place.
9. An `rw` area is a place for anything the user later runs on the host. `.git/hooks` and
   `.git/config` are read by the user's own `git`; the isolation cannot prevent that, only a
   look at the diff can.
10. 32-bit and x32 binaries do not run inside: the seccomp filter ends any process that makes a
    system call for another architecture or with the x32 bit set.
11. Deleting the worktree's `.git` from inside can make the next launch derive the worktree from
    an ancestor repository. The home directory and its ancestors are refused as a worktree;
    ancestors below that are not.
12. A main worktree made with `git init --separate-git-dir` (`.git` is a regular file whose
    target has neither `commondir` nor `core.worktree`) matches neither of the two verified
    layouts and stops with `path`.
13. The check on redirected items is made against the writable items of the current launch. An
    item written literally below the worktree (`rw = ["${worktree}", "~/work/a/b"]`) can have
    `~/work/a` swapped for a link while the worktree is `~/work`, and a later launch with a
    different worktree does not see that and applies `rw` to the link's target. The same holds
    when the item's own path is swapped for a link (`~/work/a/b` replaced, then
    `--workspace ~/work/a/b/inner` given from elsewhere on the next launch). Launched from the
    same worktree, both stop. Closing this would need remembering the previous launch, which
    `kakoi` does not do; write subdirectories of the worktree with variables.
14. `ro` and `hide` items inside an `rw` area protect less than they seem to. An `ro` written as
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
15. The shim template does not know which of the wrapped command's options take a value, so a
    word equal to the name of an option it copies is read as that option wherever it stands,
    as the value of another option or as a positional argument, and `--workspace` or `--rw`
    widens to the word that follows it: `codex -m --cd /etc exec` sends `kakoi`
    `--workspace /etc`. This misreading is accepted; the other misreadings of the template
    only leave a directory out.
16. From inside a `host` isolation whose policy sets `network.allow-nested-filtered = true`,
    persistent tun and tap devices in the host's network namespace that have no owner set, or
    are owned by your user, can be read and written.
17. The `listed` command mode stops unlisted programs from starting with Landlock, as a
    guardrail and not a boundary. It does not stop the dynamic linker started by its absolute
    path from running a readable file as a program, an allowed interpreter (`python3`, `node`)
    from loading a readable file into executable memory, or an executable copied into a
    `memfd` from starting.
18. Inside an isolation whose command mode is `listed`, `--nested=isolate` cannot make an
    isolation: Landlock forbids mounting there.
19. A program with a guard laid over its own path by `guard-absolute-path` cannot be used inside
    a sandbox that makes `/dev` anew within the isolation (such as the sandbox of an agent's CLI):
    there the guard's table and the real program are gone, and the guard stops with `guard`.
    Turn that sandbox off, or leave `guard-absolute-path` out.

## Not supported yet

- cgroup limits on CPU or memory
- multicast and broadcast traffic from the isolation
- DNS over HTTPS as the resolver's upstream
- a removal operator in the layer merge
- an automatic merge of `default.toml` under another profile
- policy files found from the current directory
- `bwrap --new-session`
- aarch64, 32-bit, and x32 binaries
- protection of `.git/hooks` and `.git/config`
- stopping every way of running a program written or downloaded into a writable place (the
  `listed` command mode stops starting it directly, but not the ways of known gap 17)
- tool-section values for any command other than codex (fill in your own in a copy of
  [`examples/shim/codex`](../examples/shim/codex))
- `init --force` (remove the file first)
- an installer for the setup skill (use your agent CLI's own means, such as
  `gh skill install ba0918/kakoi kakoi-setup`)

The reference guide's [scope chapter](guide/reference/01-overview.md) (Japanese) is
authoritative for this list.
