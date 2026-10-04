# Project Context

## Purpose

`kakoi` runs a command inside a bubblewrap (`bwrap`) mount namespace shaped by a
layered policy, and returns the command's exit code unchanged. Its network mode is `host`,
`none`, or `filtered` (kakoi-net: outbound traffic limited to allowed destinations and fixed
ports published to the host's localhost, built on pasta and nftables). It is a Rust
command-line tool for Linux on x86_64.

The IR under [`docs/ir/`](docs/ir/) (`core/` for the existing product, `network/` for
kakoi-net) is the canonical source for product, implementation, verification, and release
requirements. [`docs/guide/`](docs/guide/README.md) is its human-readable reference, in
Japanese; it adds no rules of its own. The scope of the first kakoi-net release is
`docs/ir/network/network-initial-release.md`. Rules the retired `docs/spec/` stated but the IR
does not are recorded as FLAG-011 to FLAG-026 in `docs/ir/FLAGS.md`, and the guide's appendix
`docs/guide/appendix/spec-sections.md` maps the old section numbers still cited in code and
tests to guide chapters.
[`CONTEXT.md`](CONTEXT.md) is the glossary: the project's reading of terms such as "policy",
"layer", "workspace", and "worktree", and the words not to use for them.

## Implementation and verification

The project is implemented in Rust 2021 with a minimum supported Rust version of 1.88. The
workspace manifest and locked dependency graph are in `Cargo.toml` and `Cargo.lock`; the
implementation is in `src/` (the `kakoi` binary: the command line, the start-up, and the
plan's text forms) and the responsibility-specific crates under `crates/`: pure `kakoi-policy`
and `kakoi-plan`, OS operations in `kakoi-linux`, networking in `kakoi-net`, and orchestration
and the public embedding API in `kakoi-runtime`. Behavior coverage is in `tests/` and runtime
unit tests. The public entrypoints are runtime and policy; see
[`docs/guide/maintainer/library-api.md`](docs/guide/maintainer/library-api.md).

Run the locally reproducible checks with the repository's locked dependencies:

```text
cargo build --workspace --locked
cargo test --workspace --all-targets --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --doc --locked
```

The three standalone consumers are `examples/library-policy`, `examples/library-sync`, and
`examples/library-async`. Each has its own workspace and lockfile. For each example, run:

```text
cargo build --manifest-path examples/<example>/Cargo.toml --locked
cargo test --manifest-path examples/<example>/Cargo.toml --all-targets --locked
cargo fmt --manifest-path examples/<example>/Cargo.toml --all --check
cargo clippy --manifest-path examples/<example>/Cargo.toml --all-targets --locked -- -D warnings
cargo run --manifest-path examples/<example>/Cargo.toml --locked -- --self-test
```

CI runs the workspace, documentation, and consumers on GNU and musl, and builds all four
workspaces with Rust 1.88. The API integration tests compile independent consumers in separate
target directories under this checkout to avoid reentering Cargo's outer build lock.

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
util-linux, so that their addresses and ports never meet the real host's. That namespace's root
cannot map root to itself without capabilities, which a kakoi nested inside a `filtered`
isolation has to do, so the tests that nest `filtered` in `filtered` start the outer kakoi as
user 1000 in a user namespace of its own (`unshare --map-user`), as on a real host. The verified build is Debian trixie-backports
`passt 0.0~git20260728.f8df3f1-1~bpo13+1`, which CI fetches and checks by digest.

The tests that start the built binary need `bwrap` 0.9.0 or later, `git` 2.x, and `python3` on
the machine; they fail rather than skip when any of them is missing. The tests of the "listed" modes
also need Landlock in the kernel with the scope on abstract UNIX sockets (ABI 6, Linux 6.12 or
later), and fail rather than skip without it. Inside the isolation they start `/usr/bin/cat`,
`/usr/bin/ls`, `/usr/bin/touch`, `/usr/bin/readlink`, `/usr/bin/ln`, `/usr/bin/id`, and
`/usr/bin/true` by absolute path, `git`, `id`, and small `/bin/sh` scripts by name through
`PATH`, and the `git` found on `PATH` by its absolute path. Their homes and workspaces are placed under the build's own temporary directory
(`CARGO_TARGET_TMPDIR`) rather than under `/tmp`, which the "listed" mount mode replaces; a
workspace there is given a `.git` directory of its own so that the repository the build is in
is not taken as its worktree. The build's target directory must therefore not be under
`/tmp` itself (as with a `CARGO_TARGET_DIR` there): those tests then fail. A few of them make
a directory under the host's `/tmp` or `/dev/shm`, which the isolation replaces with its own.
Inside the isolation they
start `/usr/bin/python3`, `/usr/bin/git`, `/bin/sh`, and `/bin/true` by absolute path, and
`stat`, `cat`, `test`, `rm`, `mv`, and `mkdir` through the shell. A nested run is tested for real:
a shell inside an isolation of the built `kakoi` starts the built `kakoi` again by its absolute
path (or a copy of it placed in a temporary directory, so that the nested run is a different file
from the outer run's guards), through `/usr/bin/env` when it gives the nested run another `PATH`, `HOME`, or
`XDG_CONFIG_HOME`, and the nested run starts `/usr/bin/env`, `/usr/bin/tr`, `/bin/sh`,
`/bin/echo`, `/bin/cat`, and `/usr/bin/git`. The host without `/dev/net/tun` that a nested run
meets is made the same way, inside an outer isolation that does not show the device; the tests
that show it need the host's `/dev/net/tun`. Tests that use the shared file place point
`XDG_RUNTIME_DIR` at a temporary directory; the others leave it unset. Copies of `/bin/echo` and `/bin/cat` placed in a
temporary directory are started by the relative name `-x/tool`, and a copy of `/bin/sh` there by
the name `sh` through the host's `PATH`; no copy is started by an absolute path (`python3` makes
the raw system calls that observe the seccomp filter and the socket connections that observe
the network mode). The command guard tests write small `/bin/sh` scripts into a temporary
directory and start them by name through `PATH` (`git`, `git2`) and by their absolute
path (with and without `guard-absolute-path`); they also start `sh` through `PATH`, a copy of the
built `kakoi` placed in a temporary directory under the name `git` by its absolute path, the
built `kakoi` itself by its absolute path for a nested `--print-plan=json`, and
`/usr/bin/python3` to try writing into the guards' directory. One of them starts `bwrap` inside the isolation, with a `/dev` of its own, to meet a guard
that cannot read its table. Tests point `HOME` and
`XDG_CONFIG_HOME` at a temporary directory, hand the binary only `PATH` from the developer's
environment, and never read the developer's real configuration directory.

Formatting and lint are enforced by a pre-commit hook managed by lefthook. Each Rust hook
command is wrapped in `run-if-present`, installed with `mise` as `github:ba0918/run-if-present`;
it must be on `PATH` for the hook to run. `kotowari check` runs unguarded, so `kotowari` and
`jq` must be on `PATH` too: a commit stops on any error except the test-side ones, and a push
stops unless the check exits 0 (the comment in `lefthook.yml` lists which kinds are test-side
and why). A commit that stages a file under `site/` also runs
`site/node_modules/.bin/oxfmt --check site`; install it once per clone from the lockfile with
`npm ci --prefix site --ignore-scripts` (Node.js `^20.19.0 || >=22.12.0`). Install
the hook once per clone:

```text
lefthook install
```

## Workflow: kotowari

This repository uses kotowari (`.kotowari/config.yaml`). Which kotowari skill to read, and
when, is in the routing table of `AGENTS.md`.

- The IR under `docs/ir/` is the canonical specification. Keep existing behavior; a
  brainstorm names every requirement it changes. IR not yet approved is a draft.
- The guide under `docs/guide/` is listed in `guides.files`. Each section carries guide marks
  naming the IR items it explains; when `kotowari check` reports `guide_stale`, review that
  section against the IR before copying the new fingerprint.
- Decision records go in `docs/decision/brainstorm/` and implementation plans in `docs/plans/`.
  The project keeps no ADRs: kotowari has no way to leave `decisions.adr` unset, so
  `.kotowari/config.yaml` points it at the decision records directory rather than at an empty
  directory git does not keep, which would stop `kotowari check` in a fresh clone.
- The tests kotowari reads are set in `.kotowari/config.yaml` (root integration tests and
  runtime source unit tests). If a change moves tests elsewhere, update that file too.
- Mark a test only with the requirements it actually verifies.

### Change review

`changes.files` covers implementation, tests and fixtures, examples, distributed skills,
validation tools, build configuration, hooks, CI, and agent operating instructions.
`changes.exclude` names generated outputs; lockfiles remain in scope. The selected config,
IR, decision records, and change records are automatically outside change enumeration.
The operating requirements are in [change-review.md](docs/ir/change-review.md).

The caller fixes the approved scope, worktree, phase, full base ID and full head ID from
the branch or CI event, never from a record. For a branch targeting main, fetch main and
derive the branch-wide comparison:

```sh
git fetch origin main
head=$(git rev-parse HEAD)
base=$(git merge-base origin/main "$head")
```

Keep these IDs with the review request. For a different target, substitute that target
branch. For direct integration, explicitly agree the comparison base before review.
Do not substitute HEAD's parent or infer a new branch's base from an empty push event.
An absent or unreadable history stops the check; fetch the missing history rather than
falling back to working-tree content.

The implementer writes `.kotowari/changes/implementation.yaml`. A reviewer separate from
implementation checks the grounds, IR meaning and delegated scope, then writes
`.kotowari/changes/review.yaml` for the same base, changed bytes and all implementer-related
IR. Each file contains only the current comparison. No dated histories or README belong
there; Git keeps history. `.ignore` hides these files from ordinary exploration, not Git.
Read named files explicitly or use `rg --no-ignore` when reconciling.

File identities are SHA-256 of the six-character Git mode, a NUL byte and the blob bytes.
Related IR identities are SHA-256 of the entire raw IR file. Hash the selected snapshot,
not an unstaged working-tree substitute. Record shape and decision references follow
the installed kotowari changes contract.

An optional self-check of explicitly staged implementation and its record is:

```sh
kotowari changes --base HEAD --staged --phase implementation --format json
```

This is not a gate and cannot replace independent review. Intermediate commits need no
change records, and neither lefthook hook runs `changes`. Before integration, stage and
commit both final records with the reviewed candidate, pin its full head ID again, and run:

```sh
kotowari check --format json
kotowari changes --base "$base" --head "$head" --phase review --format json
```

Both commands must exit 0. A machine pass does not prove independent review or replace
the product checks above. Deferred changes cannot pass final review. A `status` complete
value is not final change conformance.

After code, related IR or decision-meaning changes, rebase, cherry-pick, or parallel
integration, delete both invalidated final records and reconcile each affected entry in
full. Reauthor both roles after independent review, commit, pin the new head, and rerun
the relevant tests and both commands. Another base's records do not cover this comparison.

The PR-only `Kotowari` workflow checks the event's actual head, not GitHub's synthetic
merge commit, against the merge-base of the event's base and head after fetching history.
It runs the same two commands. Existing push CI does not run `changes`, because its
before/after pair can differ from the reviewed branch base. Direct pushes therefore need
the explicit local final review above; no push comparison fallback is provided.
CI installs kotowari 0.3.0 with its archive digest. Use that version locally for the same
result and reverify the gate when updating it.

## Project constraints

- `docs/ir/core/core-runtime.md` (old section 14) is authoritative for runtime boundaries: no
  persistent state. Host/none retain the `bwrap`-only execution boundary. For filtered, its
  dependencies and supported environment are governed by the proof gate in
  `docs/guide/maintainer/library.md`, including the product integration checks required
  before the initial filtered release. A launch that wraps a command (including
  `--print-plan`) writes no files, with one exception: a launch that is not nested, and not
  `--print-plan`, makes the shared file place `$XDG_RUNTIME_DIR/kakoi/` and its two files of
  fixed content (the empty file and `nameserver 127.0.0.53`) again when they are missing or
  wrong (`docs/ir/core/core-runtime.md` REQ-305, `core-nested-isolation.md` REQ-461). `init` is
  the only form that writes other files, and only within the paths the runtime boundary allows.
- `docs/ir/core/core-product.md` (old section 18) is authoritative for features the current version
  excludes.
- The version lives in `Cargo.toml` only (`docs/ir/core/core-release.md`, old section 17).
