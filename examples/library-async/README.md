# Tokioへのパイプ接続の例

runtimeのパイプをOwnedFdへ移し、FileのO_NONBLOCKを設定してTokioのAsyncFdへ渡す独立パッケージである。
Tokioは利用側だけの依存で、製品ライブラリのFutureはTokioに依存しない。
libcもこの例でFDのフラグを設定するために直接使う。

リポジトリのルートで実行する。

```sh
cargo build --manifest-path examples/library-async/Cargo.toml --locked
cargo run --manifest-path examples/library-async/Cargo.toml --locked -- --self-test
```

同期mainで`dispatch_helper()`を呼んでから、current-threadランタイムを構築する。
合成したhomeとworkspaceでcatを起動し、1 MiBの非同期書込み、読込み、終了待機を並行して行う。
入力端を閉じてEOFを送り、同期waitと同じ結果になることも確認する。

Linux x86_64、ユーザー名前空間、bwrapの`--bind-fd`と`--ro-bind-fd`が必要である。
動的リンクした例では、補助役に必要な動的リンカと共有ライブラリを隔離内から見せる。
実装は[src/main.rs](src/main.rs)、所有権とキャンセルの条件は[公開APIガイド](../../docs/guide/maintainer/library-api.md)を参照する。
