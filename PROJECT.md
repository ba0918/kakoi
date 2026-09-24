# Project Context

## Purpose

`kakoi` runs a command inside a bubblewrap (`bwrap`) mount namespace shaped by a
layered policy, and returns the command's exit code unchanged. Its network mode is `host`,
`none`, or `filtered` (kakoi-net: outbound traffic limited to allowed destinations and fixed
ports published to the host's localhost, built on pasta and nftables). It is a Rust
command-line tool for Linux on x86_64.

The specification index, [`docs/spec/kakoi.md`](docs/spec/kakoi.md), and its linked
responsibility-specific documents under `docs/spec/kakoi/` are the canonical source
for product, implementation, verification, and release requirements. The scope of the first
kakoi-net release is `docs/spec/kakoi/network/initial-release.md`.
[`CONTEXT.md`](CONTEXT.md) is the glossary: the project's reading of terms such as "policy",
"layer", "workspace", and "worktree", and the words not to use for them.

## Implementation and verification

The project is implemented in Rust 2021 with a minimum supported Rust version of 1.88. The
workspace manifest and locked dependency graph are in `Cargo.toml` and `Cargo.lock`; the
implementation is in `src/` (the `kakoi` binary: the command line, the start-up, and the
plan's text forms) and `crates/kakoi-core/src/` (the library `kakoi-core`: everything else,
usable from Rust without the command line), with behavior coverage in `tests/`.

Run the locally reproducible checks with the repository's locked dependencies:

```text
cargo build --workspace --locked
cargo test --workspace --all-targets --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

The network tests additionally need `nftables` (tested with 1.0.9) at `/usr/sbin/nft`
and `iproute2` at `/usr/sbin/ip`. The latter constructs private veth test fixtures;
it is not a product runtime dependency.
Watchdog fault tests also use `/usr/bin/nsenter` from util-linux to observe a
private namespace while its controller process is stopped or killed.
TLS transport tests use `/usr/bin/openssl` to generate temporary test certificates
and Python's `ssl` module as an independent TLS server. OpenSSL is a test dependency;
the product uses rustls and the host CA store.
They run in private user/network namespaces and do not modify host rules. These kernel
gate tests do not require TUN. The tests that carry real traffic through pasta need
`/dev/net/tun` and a pasta executable, taken from `KAKOI_TEST_PASTA` or else from `PATH`;
they fail rather than skip without one. They run kakoi inside a private
user, network, and PID namespace that stands in for the host, created with `unshare` from
util-linux, so that their addresses and ports never meet the real host's. The verified build is Debian trixie-backports
`passt 0.0~git20260728.f8df3f1-1~bpo13+1`, which CI fetches and checks by digest.

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

## Workflow: kotowari

This repository uses kotowari (`.kotowari/config.yaml`). Which kotowari skill to read, and
when, is in the routing table of `AGENTS.md`.

- The specification's canonical source stays `docs/spec/kakoi.md` and its linked documents.
  The IR under `docs/ir/` (`core/` for the existing product, `network/` for kakoi-net) is the
  checkable counterpart of that specification and must not become a second source of truth.
  Keep existing behavior; a brainstorm names every clause it changes. IR not yet approved is
  a draft.
- Decision records go in `docs/decision/brainstorm/`, ADRs in `docs/decision/adr/`, and
  implementation plans in `docs/plans/`.
- The tests kotowari reads are set in `.kotowari/config.yaml` (currently `tests/*.rs` and
  `tests/**/*.rs`). If a change moves tests elsewhere, update that file too.
- Mark a test only with the requirements it actually verifies.

## Project constraints

- The specification's section 14 is authoritative for runtime boundaries: no persistent state.
  Host/none retain the `bwrap`-only execution boundary. For filtered, its dependencies and
  supported environment are governed by `docs/spec/kakoi/proof-gate.md`, including the product
  integration checks required before the initial filtered release. A launch that wraps a command (including
  `--print-plan`) writes no files; `init` is the only form that writes files, and only within
  the paths section 14 allows.
- The specification's section 18 is authoritative for features excluded from version 0.3.
- The version lives in `Cargo.toml` only (specification section 17).
