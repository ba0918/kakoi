# 同期実行とパイプの例

`kakoi-runtime`だけを直接依存にする独立パッケージである。
同期mainで補助役を振り分け、メモリ上のポリシーでコマンドを起動する。
自己テストは合成したhomeとworkspaceを使い、パイプの出力と終了結果を確認する。

リポジトリのルートで実行する。

```sh
cargo build --manifest-path examples/library-sync/Cargo.toml --locked
cargo run --manifest-path examples/library-sync/Cargo.toml --locked -- --self-test
```

Linux x86_64とユーザー名前空間が必要で、bwrapには`--bind-fd`と`--ro-bind-fd`が必要である。
動的リンクした例では、隔離内から動的リンカと共有ライブラリが見える必要がある。
この自己テストはnoneモードで、pastaとTUNは使わない。

[src/main.rs](src/main.rs)の`self_test_standalone`が基本例である。
同じパッケージの追加fixtureは[公開API統合試験](../../tests/library_api.rs)から合成環境で呼び、停止、ガード、入れ子、外部で用意したPTYを検証する。
補助役のコピー量、main前の初期化、Dropとwaitの違いは[公開APIガイド](../../docs/guide/maintainer/library-api.md)を参照する。
