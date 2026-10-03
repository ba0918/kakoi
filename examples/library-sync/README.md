# Synchronous run with pipes

A standalone package whose only direct dependency is `kakoi-runtime`. A synchronous `main`
calls `dispatch_helper()` first, then starts a command under an in-memory policy. The self-test
uses a synthetic home and workspace and checks the piped output and the exit result.

Run from the repository root:

```sh
cargo build --manifest-path examples/library-sync/Cargo.toml --locked
cargo run --manifest-path examples/library-sync/Cargo.toml --locked -- --self-test
```

Requirements: Linux on x86_64, user namespaces, and a `bwrap` with `--bind-fd` and
`--ro-bind-fd`. When the example is linked dynamically, its dynamic linker and shared libraries
must be visible inside the isolation. The self-test uses the `none` network mode, so it needs
neither pasta nor TUN.

The basic example is [src/basic.rs](src/basic.rs). The other arguments that
[src/main.rs](src/main.rs) accepts are fixtures that the
[API integration tests](../../tests/library_api.rs) run in synthetic environments to check
stopping, guards, nesting, and an externally prepared PTY. Helper copy cost, initialization
before `main`, and the difference between dropping and waiting are covered in the
[API guide](../../docs/guide/maintainer/library-api.md) (in Japanese).
