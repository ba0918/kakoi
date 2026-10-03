# Changelog

## [Unreleased]

## [0.7.0] - 2026-10-03

### Added

- An experimental Rust embedding API. External Rust packages can depend on `kakoi-runtime`
  through a path dependency to prepare, start, stop, and wait for an isolation from their own
  process, with finite events and synchronous or runtime-independent asynchronous waits;
  packages that only build and validate policies can depend on `kakoi-policy`. Policies can be
  built from Rust types or read from TOML, and both go through the same validation as the CLI.
  The packages are not published on crates.io, and the API may change between releases.
  Embedded runs need a `bwrap` with `--bind-fd` and `--ro-bind-fd`; the CLI's requirements do
  not change. See [Rust embedding API](README.md#rust-embedding-api) and the standalone
  [policy](examples/library-policy/README.md), [sync](examples/library-sync/README.md), and
  [async](examples/library-async/README.md) examples.

### Changed

- The internal `kakoi-core` crate is split into `kakoi-policy`, `kakoi-plan`, `kakoi-linux`,
  and `kakoi-runtime`, and the CLI is built on them. The CLI's behavior, options, output, and
  exit codes are unchanged.

## [0.6.0] - 2026-10-01

### Added

- `mounts.mode = "listed"` shows only the base (`/usr`, `/bin`, `/sbin`, `/lib`, `/lib64`,
  `/etc`, read-only) and the places the policy lists; every other path, such as the sockets
  under `/run` and the rest of the home, is not there inside. The directories on the way are
  made read-only, links on a listed path are made again, `/tmp` is an empty one of the
  isolation's own, and the file `/etc/resolv.conf` points at outside the base is shown.
  `mounts.system = false` leaves the base out. With the `host` network, abstract UNIX sockets
  made outside are cut with Landlock (ABI 6); a host without it stops with `bwrap`. Once a layer
  writes `listed`, an upper layer cannot go back. See
  [Showing only what is listed](docs/policy.md#showing-only-what-is-listed-mountsmode).
- `commands.mode = "listed"` with `commands.allow` lets only the listed programs (and the
  dynamic linker and `kakoi` itself) start inside, with Landlock: `kakoi` becomes the
  isolation's first process and restricts execution before it starts the command. It is a
  guardrail, not a boundary (known gaps 17 and 18); a host without Landlock stops with `bwrap`.
  See [Allowing only listed programs](docs/policy.md#allowing-only-listed-programs-commandsmode).
- A command guard rule can have `only`: a run whose words match none of its sequences is
  denied, after the `deny` forms, rule by rule. See [Command guards](docs/policy.md#command-guards).
- `kakoi init NAME --example listed` writes out the bundled
  [`examples/profile/listed.toml`](examples/profile/listed.toml); `--example default`, or no
  `--example`, writes the built-in default as before.
- `--print-plan` shows the mount mode and `mounts.system`, and the command mode with the number
  of programs allowed; `--print-plan=json` gains `policy.mounts_mode`, `policy.mounts_system`,
  `policy.commands_mode`, `commands_allowed`, `skipped_command_allow`, and `not_shown`, and the
  roles `base` and `resolver-target` in `skipped_paths`; `format_version` stays `1`.

- `--nested=exec|isolate` chooses what a nested run does. `exec`, the default, runs the command
  under the outer isolation as before. `isolate` reads and checks the policy and makes an
  isolation inside the outer one, whose limits still hold there; when it cannot be made (a
  secret the outer isolation shows empty, no `/dev/net/tun` for `filtered`) the command does
  not run. The command guards of the outer run are shown inside as they are, and an inner
  policy that places guards of its own stops with `policy`. See
  [Nesting](docs/cli.md#nesting).
- `--print-plan` marks a nested `--nested=isolate` plan as applied inside the outer isolation,
  and `--print-plan=json` gains `applied` (whether the plan is used) and
  `policy.allow_nested_filtered`; `format_version` stays `1`.
- `network.allow-nested-filtered = true` shows the host's `/dev/net/tun` inside, in any mode,
  so that a `kakoi` nested there can make `filtered`; the plan's summary says so. It is off by
  default, and a host without the device stops the launch with `bwrap`. See
  [Nested filtered](docs/policy.md#nested-filtered).

### Changed

- **Breaking**: a nested run is told by a mark inside the isolation, an empty read-only file at
  `/dev/kakoi-isolated` that every isolation now carries, and no longer by `KAKOI=1`. A process
  inside that removes `KAKOI` still starts a nested run, and `KAKOI=1` set on the host no longer
  skips the isolation: the command is isolated, without the nesting warning. A `kakoi` started
  inside an isolation of an earlier version, which carries no mark, is not nested. `KAKOI=1` is
  still set inside the isolation, as a hint for the programs there. The plan's `nested:` line
  now says "inside an isolation" instead of naming the variable.
- A hidden file and, in `filtered` mode, `/etc/resolv.conf` are bound read-only from real files
  in the shared file place `$XDG_RUNTIME_DIR/kakoi/`, so that a `kakoi` nested inside can mount
  over them again. `kakoi` makes the place (0700) and its files (0600, empty or
  `nameserver 127.0.0.53`) right before `bwrap` starts, and makes a file again when it was
  changed; these are the only files a launch that wraps a command writes on the host, and
  `--print-plan` writes none. A nested launch, and one without a usable place (`XDG_RUNTIME_DIR`
  missing or relative, or a place that is a link, not yours, or of another mode), puts the same
  files from memory as before, without a warning. Inside, both look as before.
- A launch that uses the shared file place stops with `path` when the place is inside an `rw`
  or `rw-file` item, or such an item is inside it, as it does for a policy file.
- The resolver of a `filtered` isolation also answers at `127.0.0.54`, where a nested `kakoi`
  that follows the isolation's resolver configuration sends its questions; that configuration
  still names `127.0.0.53` alone.
- A command is no longer found where the isolation cannot start it. Looking it up on `PATH`
  passes over a name whose place, real file, or a link on the way is hidden (as a command guard
  already did), and a command given by a path hidden that way is `command not found` (127). It
  used to be picked and then fail at `bwrap`'s `exec` with exit code 1. Known gap 1 is gone, and
  the other known gaps are numbered one lower.
- The command guards also pass over a name reached through a link in a hidden place.
- A scan hit that is a link into several nested `ro` items stops the launch with `path` when
  any of them could be re-pointed from inside; only the outermost one used to be looked at.
- `guard-absolute-path` no longer lays a guard over a real program whose file has another name
  than the program (`gh` reached as a link to `mise`), which applied the rule to every program
  started through that file; the plan shows `not relocated` with the reason, and
  `--print-plan=json` gains `not_relocated` in `guards`.
- A guard that cannot read its table (inside a sandbox that makes `/dev` anew) stops with
  `guard` and exit code 126 instead of running as `kakoi`, which answered `git --version` with
  its own version. This is known gap 19.

### Fixed

- In `filtered` mode, a DNS question signed with TSIG is sent upstream with its own ID. The
  resolver replaced the ID of every question, which broke the signature, so signed questions
  failed.
- In `filtered` mode, an answer passed to the isolated process carries only address records of
  the name asked (at the end of its CNAME chain) with addresses the rules allow. An address
  record of an unrelated name that shared an allowed address, and an address record in the
  authority or additional section with an address no rule allows, used to be passed on.
- In `filtered` mode, questions asked right after the host's resolver configuration changed no
  longer fail together with an earlier question that failed under the old configuration.

### Security

- In `filtered` mode, a question still being answered when the host's resolver configuration
  changes is asked again under the new configuration. An answer from the old configuration
  could be returned to the isolated process, and the addresses in it allowed, after the new
  configuration had taken effect.
- A launch stops with `path` when an `rw` or `rw-file` item is the configuration directory's
  real path or inside it, such as `--rw ~/.config/kakoi/profile`. It used to pass when the
  policy files read were elsewhere (with the built-in default, for one), and a profile or a
  secret file planted from inside was read on the next launch.
- The bundled profile (`examples/profile/default.toml`, what `kakoi init` writes) now hides
  `/mnt/wslg`. On WSL2 with WSLg, `/mnt/wslg/distro` mounts the distro's root a second time and
  `/mnt/wslg/run/user` mounts the session's `/run/user` a second time; since `hide` acts only on
  the path it names, the credential directories and the session bus the profile hides were
  readable through them from inside the isolation. A profile written by an earlier `kakoi init`
  keeps the hole: add `"/mnt/wslg"` to its `hide` list.

## [0.5.0] - 2026-09-26

### Added

- Command guards: a rule under `[[commands.guard]]` stops one way of using a program the
  isolated process starts — `git push`, say — while the rest of the program stays usable. For
  each program a rule names, `kakoi` puts a guard (its own executable, read-only) first on
  `PATH`, in front of the program a shell inside would start by that name. A run the rules deny
  prints `kakoi: guard: <program> <the words that matched>: <reason>` on standard error, nothing
  on standard output, and exits 126; any other run is handed to the real program with the same
  arguments, environment, and working directory. A command given to `kakoi` directly goes
  through the guard too. See [Command guards](docs/policy.md#command-guards).
  - A rule denies leading words (`deny`, after skipping the global options it declares in
    `options-with-value`), flags anywhere before `--` (`deny-flags`, bundles and `--flag=value`
    included), option values (`deny-option-values`), and set environment variables
    (`deny-env`); `for` limits the last three to some leading words. A word written between two
    `/` is a regular expression.
  - `examples.deny` and `examples.allow` are checked when the policy is read, and a rule whose
    examples do not hold stops the launch with `policy`, as does a rule of the wrong shape. Rules
    from every layer apply together.
  - `guard-absolute-path = true` also lays a guard over the real program's own path, so that
    `/usr/bin/git push` is caught; a program that finds its resources from its own location can
    break under it.
  - A guard is a guardrail against mistakes, not a boundary: a process that means to get around
    it can. [Security model](docs/security.md#command-guards-are-not-a-boundary) lists what it
    cannot see. To stop something for certain, use a token with narrower permissions or
    `filtered` rules; to keep a program from being used at all, `hide` it.
  - The bundled profile carries a commented rule for `git` that denies `push` and the `-c` and
    `--config-env` aliases that would reach it.
- `--print-plan` shows the programs given a guard and those skipped, with the reason;
  `--print-plan=full` also shows the merged rules and where each came from. `--print-plan=json`
  gains `guards`, `skipped_guards`, and `policy.guards`; `format_version` stays `1`, as these
  are additions.
- The `command not executable` diagnostic (126) now also reports a guard that cannot start the
  real program, outside a nested run too.

## [0.4.0] - 2026-09-25

### Added

- A third network mode, `filtered`: new connections leave the isolation only when their
  destination, protocol, and port match a rule under `[[network.allow]]`, and a failure of the
  enforcement blocks all traffic rather than letting it through. It is chosen only by writing
  `network.mode = "filtered"`; `host` stays the default, and a policy that writes the new
  network settings without any `mode` stops before the launch. `filtered` needs `pasta` (from
  the `passt` package), `nft`, and `/dev/net/tun`; on Ubuntu 24.04 and later it also needs the
  unprivileged user namespace restriction turned off. `host` and `none` need none of them.
  - Destinations are an IP address, a CIDR, a DNS name (`example.com`, or `*.example.com` for
    every name below it), or `host-loopback` to reach a service on the host's own loopback
    through `host-v4.kakoi.internal` and `host-v6.kakoi.internal`.
  - Names resolve through a resolver `kakoi` runs, which checks every answer and allows an
    address only while the answer's time to live lasts. It follows the host's
    `/etc/resolv.conf`, including changes while running, unless `[[network.dns-upstream]]`
    names plain or TLS upstreams.
  - `[[network.publish]]` with `mode = "fixed"` publishes a port inside the isolation to the
    host's `127.0.0.1` or `::1`, taken before the command starts and announced on standard
    error.
  - When the main command ends, the network is stopped first, and the processes left behind get
    one common grace, `process.shutdown-grace-seconds` (default 5), before they are killed.
  - IPv6 link-local destinations (`host-interface`) are not available yet: a rule with one is
    refused before the launch.
- When `pasta` is missing from `PATH`, or is too old for the options `kakoi` passes (Ubuntu
  24.04's own is), the launch stops with a `bwrap` diagnostic that names the missing options and
  points to [Installing pasta](docs/pasta.md), which tells how to check the `pasta` you have and
  how to build the tested release from source into `~/.local/bin`. The setup skill checks the
  installed `pasta` the same way when you use or want `filtered`, and shows those steps; it
  installs nothing itself.
- `--print-plan=json` gains the network settings of the merged policy (`network_allow`,
  `network_publish`, `network_limits`, `dns_upstream`, `shutdown_grace_seconds`);
  `format_version` stays `1`, as these are additions.

### Fixed

- An `rw-copy` of a directory now gives the copy's top directory the permission bits of the
  source, like every entry below it.
- A path reached through exactly 40 symbolic links is now resolved by the released static
  binary as the specification states; it had been treated as missing, since the C library the
  binary links stops one link sooner.
- For programs using the library: the in-memory files holding the seccomp filter and the
  `rw-copy` contents are no longer inherited by another isolation the same process starts at
  the same time.

## [0.3.0] - 2026-09-10

### Added

- A fifth mount directive, `rw-copy`, for a path the command must be free to write and whose
  writing must not outlive the run. It takes a directory or a regular file, like `ro`: the
  isolation starts from the host's content — a directory as a tmpfs of its own filled with the
  tree at the real path (permission bits and the execute bit carried, empty directories kept,
  symbolic links reproduced as links), a regular file as a copy of its bytes bound over it —
  writes it freely, and leaves the host's own untouched. It follows the same rules as the other
  four: it is skipped when the path does not exist, the narrower item still wins, and it appears
  in `--print-plan` in every form, where the summary also names what the directive does.

  Because nothing written inside one reaches the host, an `rw-copy` area is not a writable place
  for the placement checks: a policy file, the configuration directory, or a secret file inside
  one is allowed, since the next launch reads what this one read. What is still refused is an
  `rw-copy` that could be redirected onto a `hide`, which would show what was hidden, and an
  `rw` or `rw-file` that could be redirected onto an `rw-copy`, which would let the writing
  through.

  One item carries at most 4096 entries and 64 MiB of file content, since the content is held in
  memory twice over; past either limit the launch stops with `path`. A source that cannot be read
  stops the launch with `path` rather than starting from less than the host has, and an entry no
  mount argument can recreate in a tmpfs (a socket, a FIFO, a device node) is left out and named
  in the plan.

  `--print-plan=json` gains a `not_copied` key and a `{"kind": "copied-file", "bytes": ...}`
  argument kind; `format_version` stays `1`, as both are additions.

## [0.2.0] - 2026-09-10

### Changed

- **Breaking**: the project is renamed from `process-wrap` to `kakoi`. The executable, the
  environment variables (`KAKOI` and `KAKOI_SHIM_OFF`), the configuration directory
  (`$XDG_CONFIG_HOME/kakoi/`), the shared directory (`/tmp/kakoi`), the diagnostic prefix, the setup
  skill (`kakoi-setup`), the release archive
  (`kakoi-v<version>-x86_64-unknown-linux-musl.tar.gz`), and the repository URL all use the new
  name. There is no compatibility path from the old names: an existing configuration directory,
  shim, or environment variable must be moved to the new names by hand.

## [0.1.1] - 2026-09-09

### Added

- Every release carries a statically linked binary for Linux on x86_64, as
  `process-wrap-v<version>-x86_64-unknown-linux-musl.tar.gz` with its SHA-256 beside it.
  `mise use -g github:ba0918/process-wrap` installs it, and the archive can be taken by hand
  from the release page. Installing no longer needs a Rust toolchain, and the binary links
  against nothing on the machine.

### Changed

- The install instructions lead with that binary. Building from source is still supported and
  is now `cargo install --git https://github.com/ba0918/process-wrap --locked`; the old
  `cargo install --path .` needed a clone.
- The setup skill carries its own copy of the shim template, so it no longer asks where the
  `process-wrap` source tree is. The shim template itself can be fetched from the repository
  with `curl` rather than copied out of a clone.

## [0.1.0] - 2026-09-09

The first release. `process-wrap` runs a command inside a bubblewrap mount namespace shaped by a
layered policy, and returns the command's exit code unchanged.

### Added

- A policy merged from three layers — a profile, `--policy-file`, and the command line. It names
  `rw`, `rw-file`, `ro`, and `hide` mount items with `~` and the variables `${workspace}`,
  `${worktree}`, `${git_common_dir}`, and `${config_dir}`; scans the worktree for environment
  files to hide; hides mounts by file system type; chooses the network mode; shapes the
  environment; injects secrets from files; and adds git URL rewrites.
- Placement checks that refuse a policy which could be subverted from inside the isolation: a
  policy file, configuration directory, secret file, or `path-prepend` entry that could be
  swapped; a path resolving through a writable item to somewhere outside every root item, or
  following a link inside one; an item that would expose what a lower `hide` or `ro` covers; and
  an item landing on `/`, `/dev`, or `/proc`. The home directory and its ancestors are refused as
  a worktree or an `rw` item.
- A seccomp filter that fails `ioctl(TIOCSTI)` with `EPERM`, so keystrokes cannot be pushed into
  the terminal that way, and ends processes making foreign-architecture or x32 system calls.
- `--print-plan`, which shows what a launch would do without launching: a summary for reading,
  `=full` for the merged policy, the origin of every item, and the `bwrap` argument list, and
  `=json` for tools and agents, whose keys are a contract.
- One-line diagnostics with the exit codes 125 (`process-wrap` itself), 126 (a command found but
  not executable, in a nested run), and 127 (a command not found). The command's own exit code is
  returned unchanged, and a nested run, marked by `PROCESS_WRAP=1`, executes the command without
  isolating it again.
- The bundled WSL2 profile compiled into the binary as the built-in default, so `process-wrap --
  COMMAND` works with nothing written to the configuration directory.
- `process-wrap init [NAME]`, which writes that profile to `profile/NAME.toml` and creates the
  configuration directory, `profile/`, and `secrets/` (mode 0700). It refuses to replace anything
  already at that name and has no `--force`. It is the only form that writes.
- `examples/shim/codex`, a shim template that sends every invocation through `process-wrap`,
  split into a body that does not depend on the command being wrapped and a tool section at the
  top of the file that carries everything that does.
- `skills/process-wrap-setup/SKILL.md`, an Agent Skill that proposes the machine-specific parts
  of a profile and a shim with its tool section filled in, shows a diff, and waits for approval
  before writing.

[Unreleased]: https://github.com/ba0918/kakoi/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/ba0918/kakoi/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/ba0918/kakoi/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/ba0918/kakoi/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/ba0918/kakoi/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/ba0918/kakoi/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/ba0918/kakoi/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/ba0918/process-wrap/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ba0918/process-wrap/releases/tag/v0.1.0
