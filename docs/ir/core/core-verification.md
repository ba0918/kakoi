# 検証の境界

既存仕様から継承した本体・補助物の契約。正本は人間向け仕様文書群である。

## 要求

### REQ-354: 計画算出の純粋な境界
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: review

計画算出は合成前の各段と出所、CLI引数、ホスト環境、収集済み事実だけを入力にする純粋関数にする。事実には候補パスの存在・種類・実体、マウント一覧と取得失敗、走査結果、Git相互リンク、秘密の内容、パス解決で参照したもの、rw-copyの木、cwdを含む。事実収集、記述子割当、bwrap起動は関数の外に置く。

### REQ-355: 純粋関数の検証範囲
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: review

純粋関数の検証では、仕様第15.1節の全項目を含める。対象はGitの相互リンク、合成・同一性、配置保護の拒否と許容、rw-copyの適用と生成引数、固定マウント・広さ、走査と生成hide、6場面の重なり、変数・環境・秘密・Git番号・制御文字、コマンド解決、init文法である。各対象の条件と対になる境界を同節の確認一覧に保持する。

### REQ-356: 実バイナリの検証範囲
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: review

Linux x86_64でビルド済みkakoiと実bwrapを起動し、第15.2節の全観測条件を確認する。診断・出力・終了と検査順序、FIFOとサイズ上限、1100件走査時の記述子上限、6場面のマウント、rw-copyのホスト不変性と失敗、none、秘密、seccomp、入れ子、argv[0]、init、既定プロファイル、JSONを含む。テストは外部ネットワーク接続に依存しない。

### REQ-357: 移行時の一度限りの確認
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: review

現行codex用jailからの移行時は、利用者の実機で旧スクリプトのbwrap引数とprint-planを並べ、マウント集合が一致することを人が一度確認する。旧スクリプトは製品でないため継続テストにしない。

## 具体例

```gherkin
@id=EX-648 @about=REQ-354 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 計画算出の純粋な境界・成功
  Given 合成した事実を計画算出に渡す
  When 契約への適合を確認する
  Then 外部I/Oなしで記号付き引数・環境・適用結果・コマンドを比較できる

@id=EX-649 @about=REQ-354 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 計画算出の純粋な境界・反例
  Given 合成した事実を計画算出に渡す
  When 契約への適合を確認する
  Then 計画算出がホストのファイルを直接読むことは契約違反である

@id=EX-650 @about=REQ-355 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 純粋関数の検証範囲・成功
  Given 第15.1節とテストを照合する
  When 契約への適合を確認する
  Then 列挙した成立・不成立の条件をテストで確認できる

@id=EX-651 @about=REQ-355 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 純粋関数の検証範囲・反例
  Given 第15.1節とテストを照合する
  When 契約への適合を確認する
  Then 根の検査の拒否だけ確認し対象外のroを確認しないことは契約違反である

@id=EX-652 @about=REQ-356 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 実バイナリの検証範囲・成功
  Given 実バイナリのテストを実行する
  When 契約への適合を確認する
  Then 隔離内外の観測を伴う検証がネットワークなしで通る

@id=EX-653 @about=REQ-356 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 実バイナリの検証範囲・反例
  Given 実バイナリのテストを実行する
  When 契約への適合を確認する
  Then 純粋関数だけの成功で実バイナリを検証済みにすることは契約違反である

@id=EX-654 @about=REQ-357 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 移行時の一度限りの確認・成功
  Given 旧jailから移行する
  When 契約への適合を確認する
  Then 利用者が実機のマウント集合を比較する

@id=EX-655 @about=REQ-357 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 移行時の一度限りの確認・反例
  Given 旧jailから移行する
  When 契約への適合を確認する
  Then 旧スクリプトを製品の恒久テスト依存にすることは契約違反である

```
