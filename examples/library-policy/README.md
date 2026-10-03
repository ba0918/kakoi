# ポリシーだけを扱う例

`kakoi-policy`だけを直接依存にする独立パッケージである。
Rust入力とTOMLの共通検証を試し、隔離は起動しない。

リポジトリのルートで実行する。

```sh
cargo build --manifest-path examples/library-policy/Cargo.toml --locked
cargo run --manifest-path examples/library-policy/Cargo.toml --locked -- --self-test
```

構築コードは[src/main.rs](src/main.rs)、入力の説明は[公開APIガイド](../../docs/guide/maintainer/library-api.md)を参照する。
