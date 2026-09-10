# kakoi

`kakoi` runs a command inside a [bubblewrap](https://github.com/containers/bubblewrap)
mount namespace shaped by a small TOML policy, and returns the command's exit code unchanged.
It is built for one job: running an LLM CLI (Codex, Claude Code, opencode) against one
repository without handing it the rest of your home directory, your credentials, or the sockets
that carry them.

> **Not a replacement for a container or a VM.** `kakoi` shares the host's kernel, trusts
> the host it is started from, and sets no resource limits. It narrows what an agent can see and
> touch during everyday work; it does not make an untrusted program safe to run. See
> [Security model](#security-model).

## Why

- **The boundary sits outside the CLI.** An agent CLI's own sandbox is a setting of the CLI: it
  changes with its version, its configuration, and the flags it was started with. `kakoi`
  wraps the whole process from the outside, so the boundary is yours and stays the same whatever
  the CLI decides.
- **Light enough for every launch.** One binary, one `bwrap` call, no daemon, no image, no
  persistent state. `cd` into the repository and run.
- **A trust boundary you can state in a sentence.** The host is trusted; the isolated process is
  not. The policy decides what the process can write and what it cannot see, and the exceptions
  are written down as [known gaps](docs/security.md#known-gaps) rather than left implicit.

## Requirements

- Linux on x86_64, including WSL2 (no aarch64 in 0.2)
- `bwrap` 0.9.0 or later on `PATH` (the `bubblewrap` package on Debian and Ubuntu)
- On Ubuntu 24.04 and later, permission for `bwrap` to use a user namespace: the restriction is
  on by default, and every launch needs the namespace. See
  [Allowing the user namespace](docs/getting-started.md#allowing-the-user-namespace-on-ubuntu-2404-and-later).
- A Rust toolchain, 1.85 or later, only to build from source

## Install

```sh
mise use -g github:ba0918/kakoi
```

Every release carries a statically linked binary for Linux on x86_64: nothing to build, and no
library it has to find on your machine. Without `mise`, take the archive from the
[latest release](https://github.com/ba0918/kakoi/releases/latest), check it against the
`.sha256` beside it, and put `kakoi` on your `PATH`. To build from source instead:

```sh
cargo install --git https://github.com/ba0918/kakoi --locked
```

That is the whole installation. With no profile written anywhere, `kakoi -- COMMAND`
starts on the built-in default, a profile written for WSL2 and compiled into the binary.

## Quick start

```sh
cd ~/work/project
kakoi -- codex               # run codex inside the isolation, on the built-in default
$EDITOR "$(kakoi init)"      # write the default profile out and edit it
kakoi --print-plan -- codex  # show what would be mounted, hidden, and set; run nothing
```

Two things are empty right after installing: `/tmp` is replaced by an empty directory (only
`/tmp/kakoi` is shared with the host, and you or your shim create it), and `~/.config/gh`
is hidden, so `gh` inside the isolation is not authenticated. [Getting started](docs/getting-started.md)
covers both, along with the configuration directory and how to pass a GitHub token.

## Policy example

A policy file is TOML. Every section is optional, and an empty file is a valid policy.

```toml
[mounts]
rw      = ["${worktree}", "${git_common_dir}", "~/.cache"] # writable
rw-copy = ["~/.gitconfig"]                                 # writable inside, unchanged outside
ro      = ["~/.codex/AGENTS.md"]                           # read-only
hide    = ["~/.ssh", "~/.aws", "/tmp"]                     # empty inside

[[mounts.scan]]                       # hide .env files anywhere in the worktree
root  = "${worktree}"
names = [".env", ".env.*"]

[env]
unset = ["SSH_AUTH_SOCK", "*_TOKEN", "*_API_KEY"]

[secrets]                             # the file's content becomes GH_TOKEN; the file is hidden
GH_TOKEN = "${config_dir}/secrets/gh-token"
```

The bundled [`examples/profile/default.toml`](examples/profile/default.toml) shows every section
in use, and [Writing a policy](docs/policy.md) explains each key.

## How it works

Up to three layers are merged, lowest first, and the upper layer wins:

1. the **profile**, `profile/NAME.toml` in the configuration directory (`--profile NAME`,
   default `default`);
2. a **policy file** given with `--policy-file PATH`;
3. the **command line**: `--rw PATH` and `--hide PATH`.

The merged policy names mount items with five directives:

| Directive | Inside the isolation |
| --- | --- |
| `rw` | a directory, readable and writable |
| `rw-file` | a single file, writable in place |
| `rw-copy` | a directory or a file, starting from the host's content and writable, with nothing written reaching the host |
| `ro` | a directory or a file, read-only |
| `hide` | a directory becomes an empty directory; a file reads as empty |

Everything the policy does not name is visible read-only. The narrower item wins, so
`hide = ["/tmp"]` with `rw = ["/tmp/kakoi"]` gives an empty `/tmp` with one shared
directory inside it. Paths can use `~` and the variables `${workspace}`, `${worktree}`,
`${git_common_dir}`, and `${config_dir}`.

- **`rw-copy`** is the one to reach for when a tool has to write its own configuration and you
  do not want the result on your machine: the isolation gets a copy of what is at the path,
  writes it freely, and the copy goes when the command ends.
- **Secrets** are read from files, injected as environment variables, and the files are hidden
  inside. The configuration directory's `secrets/` is always hidden.
- **Network** is either shared with the host (`host`) or cut (`none`); there is nothing in
  between.
- **Environment** is inherited or cleared, then shaped by `unset` patterns, `set`, secrets, and
  `path-prepend`. `KAKOI=1` marks the inside.

The process ID, IPC, UTS, cgroup, and user namespaces are always unshared. `--print-plan` shows
every mount item with the reason it was applied or skipped and how the environment differs from
the host's; `--print-plan=full` adds the merged policy, the final environment with secret values
masked, and the `bwrap` argument list, and `--print-plan=json` is the same as one line of JSON, for
LLM agents and tools.

## Wrapping an LLM CLI

`kakoi` knows nothing about the command it wraps. A shim on `PATH` does the wrapping:

```sh
# ~/.local/bin has to come before the real codex on PATH
curl -fsSLo ~/.local/bin/codex \
  https://raw.githubusercontent.com/ba0918/kakoi/main/examples/shim/codex
chmod +x ~/.local/bin/codex
codex   # now every invocation goes through kakoi
```

The shim finds the real `codex` further down `PATH`, copies its `--cd` and `--add-dir` values to
`--workspace` and `--rw`, and puts `--dangerously-bypass-approvals-and-sandbox` in front, since
the boundary is `kakoi`'s and not the CLI's. `KAKOI_SHIM_OFF=1` runs the real
command outside `kakoi`. To wrap another CLI, copy the template and fill in the tool
section at the top of the file. [Wrapping a command](docs/shim.md) has the details and the
conditions to check on your copy.

The [`kakoi-setup`](skills/kakoi-setup) Agent Skill fits the profile and the shim
to the CLIs installed on your machine:

```sh
gh skill install ba0918/kakoi kakoi-setup
```

Run it outside the isolation, in a mode where your CLI asks before writing; it proposes each
change as a diff.

## Security model

What `kakoi` guarantees, when the launch is not refused:

- the process sees the file system the policy describes, and nothing it hides;
- `rw` and `rw-file` are the only places whose writes the host keeps (an `rw-copy` is writable
  too, and what is written there goes when the command ends);
- the secrets and the credential files the policy names are not readable from inside;
- `ioctl(TIOCSTI)` is blocked, so keystrokes cannot be pushed into your terminal that way;
- the exit code you get is the command's own.

What it does not guarantee:

- anything about the host: `kakoi` trusts `HOME`, `PATH`, the current directory, and the
  environment it starts in;
- resource limits (no cgroups), per-domain network rules, or kernel isolation;
- that an `rw` area stays harmless afterwards: `.git/hooks` and `.git/config` in a repository
  you later use on the host are yours to review.

The full threat model, the placement checks that refuse a policy file inside a writable area,
and the fifteen known gaps are in [Security model](docs/security.md).

## Documentation

- [Getting started](docs/getting-started.md): the configuration directory, `init`, the built-in
  default, passing a token, the first launch.
- [Command line](docs/cli.md): options, `--print-plan`, exit codes and diagnostics.
- [Writing a policy](docs/policy.md): every key, paths and variables, layers, secrets, and the
  paths that are refused.
- [Wrapping a command](docs/shim.md): the shim template and the setup skill.
- [Security model](docs/security.md): trust boundary, known gaps, and what 0.2 leaves out.
- [Specification](docs/spec/kakoi.md) (Japanese): the complete behaviour;
  [`CONTEXT.md`](CONTEXT.md) is the glossary.

## Status

0.2 is the current version. Among the things it does not do: no cgroup limits, no per-domain
network allowance, no aarch64, no protection of `.git/hooks` and `.git/config`, and no shipped
tool-section values for any CLI other than codex. The complete list is in
[Not in 0.2](docs/security.md#not-in-02).

## License

MIT. See [LICENSE](LICENSE).
