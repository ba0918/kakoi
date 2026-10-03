# Pipes with Tokio

A standalone package that moves the runtime's pipes into `OwnedFd`, sets `O_NONBLOCK` on each
`File`, and hands them to Tokio's `AsyncFd`. Tokio is a dependency of this consumer only; the
library's futures do not depend on Tokio. The example also uses `libc` directly to set the
descriptor flags.

Run from the repository root:

```sh
cargo build --manifest-path examples/library-async/Cargo.toml --locked
cargo run --manifest-path examples/library-async/Cargo.toml --locked -- --self-test
```

A synchronous `main` calls `dispatch_helper()` before it builds a current-thread runtime. The
self-test starts `cat` over a synthetic home and workspace and runs a 1 MiB asynchronous write,
the read, and the wait for exit concurrently. It then closes the input end to send EOF and checks
that the result matches the synchronous wait.

Requirements: Linux on x86_64, user namespaces, and a `bwrap` with `--bind-fd` and
`--ro-bind-fd`. When the example is linked dynamically, the dynamic linker and shared libraries
the helper needs must be visible inside the isolation. The code is in [src/main.rs](src/main.rs);
ownership and cancellation are covered in the
[API guide](../../docs/guide/maintainer/library-api.md) (in Japanese).
