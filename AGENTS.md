# Agent Instructions

プロジェクト固有の契約は PROJECT.md を読む。

## kotowari

kakoi-net の壁打ちで kotowari 0.1.0 を試走中。brainstorm、plan、cycle、implement の各席では `kotowari` スキルを読み、場面に応じた reference に従う。

IR（検査できる形に整理した仕様）は `docs/ir/`、判断の記録は `docs/decision/brainstorm/` に置く。対象は kakoi-net の追加・改訂部分と、利用者が追加で依頼した本体既存仕様のIR抽出・既存テストの対応付け。既存仕様は維持し、変更する条項は壁打ちで明示する。承認前の IR は草案として扱い、仕様の正本は `docs/spec/kakoi.md` を入口とする責務別のspec文書群。IRは対応する検査用表現で、正本を増やさない。kakoi-netの責務別仕様は会話で承認済み。本体IR抽出と既存テストの対応付けも承認済み。文章の追加校正は別件とし、既存の製品挙動は変更しない。

kotowari のテスト検査対象は `tests/*.rs` と `tests/**/*.rs`。既存テストは実際に確認している要求だけに印を付ける。今回の機能のテスト配置を変える場合は `.kotowari/config.yaml` も更新する。
