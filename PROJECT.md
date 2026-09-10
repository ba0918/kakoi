# Project Context

## Purpose

`kakoi` runs a command inside a bubblewrap (`bwrap`) mount namespace shaped by a
layered policy, and returns the command's exit code unchanged. It is a Rust command-line tool
for Linux on x86_64.

The approved specification, [`docs/spec/kakoi.md`](docs/spec/kakoi.md), is the
canonical source for product, implementation, verification, and release requirements.
[`CONTEXT.md`](CONTEXT.md) is the glossary: the project's reading of terms such as "policy",
"layer", "workspace", and "worktree", and the words not to use for them.

## Implementation and verification

The project is implemented in Rust 2021 with a minimum supported Rust version of 1.85. The
package manifest and locked dependency graph are in `Cargo.toml` and `Cargo.lock`; the
implementation is in `src/`, with behavior coverage in `tests/`.

Run the locally reproducible checks with the repository's locked dependencies:

```text
cargo build --locked
cargo test --all-targets --locked
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
```

The tests that start the built binary need `bwrap` 0.9.0 or later, `git` 2.x, and `python3` on
the machine; they fail rather than skip when any of them is missing. Inside the isolation they
start `/usr/bin/python3`, `/usr/bin/git`, `/bin/sh`, and `/bin/true` by absolute path, and in a
nested run `/usr/bin/env` and `/bin/sh`. Copies of `/bin/echo` and `/bin/cat` placed in a
temporary directory are started by the relative name `-x/tool`, and a copy of `/bin/sh` there by
the name `sh` through the host's `PATH`; no copy is started by an absolute path (`python3` makes
the raw system calls that observe the seccomp filter and the socket connections that observe
the network mode). Tests point `HOME` and
`XDG_CONFIG_HOME` at a temporary directory, hand the binary only `PATH` from the developer's
environment, and never read the developer's real configuration directory.

Formatting and lint are enforced by a pre-commit hook managed by lefthook. Each hook command is
wrapped in `run-if-present`, installed with `mise` as `github:ba0918/run-if-present`; it must be
on `PATH` for the hook to run. Install the hook once per clone:

```text
lefthook install
```

## Project constraints

- The specification's section 14 is authoritative for runtime boundaries: no persistent state,
  no external command other than `bwrap`. A launch that wraps a command (including
  `--print-plan`) writes no files; `init` is the only form that writes files, and only within
  the paths section 14 allows.
- The specification's section 18 is authoritative for features excluded from version 0.2.
- The version lives in `Cargo.toml` only (specification section 17).
