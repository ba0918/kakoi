# Getting started

This page takes you from `cargo install` to a profile of your own. The [README](../README.md)
has the short version; the [command line](cli.md) and [policy](policy.md) pages have the
reference.

## Install

`process-wrap` is built from source with a Rust toolchain (1.85 or later):

```sh
cargo install --path .
```

`bwrap` 0.9.0 or later must be on `PATH`; on Debian and Ubuntu it is the `bubblewrap` package.
`process-wrap` runs on Linux on x86_64, including WSL2.

That is the whole installation. With no profile written anywhere, `process-wrap -- COMMAND`
starts on the built-in default: the bundled
[`examples/profile/default.toml`](../examples/profile/default.toml), compiled into the binary.
Where the plan would name the profile file, it says `process-wrap init` instead.

## The first launch

Two things are empty on a machine you have just installed on:

- `/tmp` is replaced by an empty directory, and the shared `/tmp/process-wrap` does not exist
  yet. Nothing passes through `/tmp` until you or your shim creates that directory;
  `process-wrap` never does.
- `~/.config/gh` is hidden, so a `gh` inside the isolation is not authenticated. See
  [Pass a GitHub token](#pass-a-github-token) below.

## The configuration directory

The configuration directory is `$XDG_CONFIG_HOME/process-wrap` when that variable holds an
absolute path, and `~/.config/process-wrap` otherwise. It holds two things:

- `profile/NAME.toml`, the profiles `--profile NAME` selects (`default` when the option is left
  out);
- `secrets/`, the place for secret files. It is always hidden inside the isolation.

The built-in default is used whenever `profile/default.toml` is simply not there. That includes
the time while `XDG_CONFIG_HOME` points into dotfiles you have not cloned yet on a new machine.
Anything else in the way, such as a broken symbolic link or a regular file where a directory
belongs, stops the launch with a `policy` diagnostic instead, so a profile that broke is never
quietly replaced by the wider default. A profile named with `--profile NAME` never falls back
either.

## Write the boundary out and edit it

`process-wrap init` writes the built-in default to `<configuration directory>/profile/default.toml`,
creates `profile/` and `secrets/` (mode 0700) and any missing ancestor, and prints the path of
the file it wrote:

```sh
$EDITOR "$(process-wrap init)"
```

It refuses to replace anything already at that name and has no `--force`: remove the file first
if you want it back. `process-wrap init NAME` writes `profile/NAME.toml`, which `--profile NAME`
then selects.

## Read what the default gives you

The default is written for WSL2. It hides the Windows drives under `/mnt`, `/run/WSL`, `/tmp`,
`/run/user`, and the usual credential directories; opens the workspace and the worktree for
writing; and drops the credential-shaped environment variables. Read the file `init` wrote, or
run

```sh
process-wrap --print-plan -- true
```

to see what it makes of the machine you are on: what is writable, read-only, or hidden, and
which environment variables change. `--print-plan=full` adds the merged policy, the whole
environment, and the `bwrap` arguments. The plan is described on the
[command line](cli.md#the-plan) page.

## Pass a GitHub token

Uncomment the `secrets` line in the profile `init` wrote:

```toml
[secrets]
GH_TOKEN = "${config_dir}/secrets/gh-token"
```

and put the token in `<configuration directory>/secrets/gh-token`, one line. `secrets/` is always
hidden inside the isolation, so the file cannot be read from in there; the value arrives as the
environment variable named on the left of the line. [Secrets](policy.md#secrets) has the rules
for the file.

## Next steps

- **Wrap a command** so that every invocation of `codex` (or another CLI) goes through
  `process-wrap`: see [Wrapping a command](shim.md).
- **Let an agent fit the profile to this machine** with the
  [`process-wrap-setup`](../skills/process-wrap-setup) skill: see
  [the setup skill](shim.md#the-setup-skill).
- **Read the placement rules** before moving the configuration directory into dotfiles: see
  [Paths that are refused](policy.md#paths-that-are-refused).
