# Glossary

外部のRustプログラムから利用する公開APIの仕様を置く。
このディレクトリの新規仕様は承認前の草案である。

| Term | Meaning | Source |
|---|---|---|
| 実行用計画 | 検証と計画生成が完了した、不変で一度の起動に消費する値。ホストのファイル内容を固定したスナップショットとは呼ばない | docs/decision/brainstorm/2026-10-03-public-library-api.md#A10 |
| 実行ハンドル | 起動した隔離の所有者を表し、停止要求、状態照会、待機を行う値。破棄すると停止処理を開始する。イベントの受信者とは区別する | docs/decision/brainstorm/2026-10-03-public-library-api.md#A6, docs/decision/brainstorm/2026-10-03-public-library-api.md#A8 |
