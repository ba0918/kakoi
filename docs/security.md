# Security model

`kakoi` narrows what a process can see and touch. It is not a replacement for a
container or a VM: it shares the host's kernel, trusts the host it is started from, and sets no
resource limits. This page states the boundary, what it guarantees, and where it is known to
leak.

## The trust boundary

The host is the trusted side; the isolated process is not. `kakoi` trusts the environment
it starts in (`HOME`, `XDG_CONFIG_HOME`, `PATH`, `KAKOI`, and the current directory),
the policy files it reads, and the `bwrap` it finds on `PATH`. Everything the host does, your
shell, your `git`, the files you open afterwards, is outside the boundary.

The isolation has four dimensions:

- **file system**: what is visible and what is writable, as the policy's mount items say;
- **network**: shared with the host or cut;
- **environment**: what is inherited, dropped, and added;
- **credentials**: files hidden and secrets injected.

The process ID, IPC, UTS, cgroup, and user namespaces are always unshared. A seccomp filter
fails `ioctl(TIOCSTI)` with `EPERM` and ends any process that makes a system call for another
architecture or with the x32 bit set. The boundary is assembled once at start-up and does not
change afterwards.

## What is guaranteed

When the launch is not refused:

- the process sees the file system the policy describes, and nothing the policy hides;
- `rw` and `rw-file` items are the only places it can write;
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
- Network policy finer than on or off.
- Kernel isolation: the process shares the host's kernel, as with any namespace-based sandbox.
- That an `rw` area stays harmless afterwards. What the process writes there (`.git/hooks`,
  `.git/config`, build scripts) is read by whatever you later run on the host.
- Complete protection of `ro` and `hide` items placed inside an `rw` area; see known gap 15.

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
8. Nesting is detected only through `KAKOI=1`. Clearing the environment inside the
   isolation and starting `kakoi` again attempts a second isolation (no wider than the
   first; a policy with secrets fails there because the outer isolation emptied the files).
   Setting `KAKOI=1` on the host runs the command without isolation, with the nesting
   warning on standard error.
9. `kakoi` trusts the environment it starts in: `HOME`, `XDG_CONFIG_HOME`, `PATH`,
   `KAKOI`, and the current directory. That includes the current directory: `cd` into a
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
    `kakoi` does not do; write subdirectories of the worktree with variables.
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

- cgroup limits on CPU or memory
- a per-domain network allowance
- a removal operator in the layer merge
- an automatic merge of `default.toml` under another profile
- policy files found from the current directory
- `bwrap --new-session`
- double isolation when nested, and nesting detection other than the environment variable
- aarch64, 32-bit, and x32 binaries
- protection of `.git/hooks` and `.git/config`
- tool-section values for any command other than codex (fill in your own in a copy of
  [`examples/shim/codex`](../examples/shim/codex))
- `init --force` (remove the file first)
- an installer for the setup skill (use your agent CLI's own means, such as
  `gh skill install ba0918/kakoi kakoi-setup`)

The specification's [section 18](spec/kakoi.md#18-01-で作らないもの) is authoritative for
this list.
