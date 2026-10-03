# Policy-only example

A standalone package whose only direct dependency is `kakoi-policy`. It builds the same policy
from Rust types and from TOML, checks that both pass the shared validation with the same result,
and starts no isolation.

Run from the repository root:

```sh
cargo build --manifest-path examples/library-policy/Cargo.toml --locked
cargo run --manifest-path examples/library-policy/Cargo.toml --locked -- --self-test
```

The code is in [src/main.rs](src/main.rs). The policy input types are described in the
[API guide](../../docs/guide/maintainer/library-api.md) (in Japanese).
