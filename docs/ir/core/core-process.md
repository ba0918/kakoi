# 入れ子と並列起動

既存仕様から継承した本体の検査用表現。同じ責務を一文書にまとめ、見出しと要求IDで参照する。この IR が正本である。

## Requirements

### REQ-284: 入れ子の実行
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

入れ子ではinitと計画表示を除き、ポリシー読込・bwrap起動・環境変更・cwdとHOMEの検査を行わず、内側プロファイルを適用しないwarningを1行出してコマンドを直接execする。

### REQ-285: 入れ子の計画
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

入れ子の計画表示はポリシーを読み計画に入れ子の印を付ける。計画のPATHは隔離環境の値、解決先はホストPATHによる値として区別する。

### REQ-286: 入れ子検出の限界
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

入れ子検出は環境変数だけに頼る。隔離内で環境を消すと二重隔離を試み、外側より広がらず、空にされた秘密はsecret診断になる。ホストKAKOI=1では隔離せず警告して実行する。両方をREADMEに記す。

### REQ-287: 並列起動
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

コマンドを包む並列起動は内部状態を共有せず、一方の終了で他方を停止しない。同じNAMEのinitの競合結果は保証しない。filteredの公開ポート競合は既存の公開割当・復帰仕様に従う。

### REQ-288: 診断と警告の形
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

診断は標準エラー1行のkakoi: <種類>: <説明>、警告はkakoi: warning: <説明>とする。種類は契約、説明は自由文。警告は終了結果に先行して出ることがある。

## Examples

```gherkin
@id=EX-520 @about=REQ-284 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 入れ子の実行
  Given 本体の既存仕様を適用する
  When KAKOI=1の環境でコマンドを実行する
  Then 受け取った環境のまま直接execしてwarningを出す

@id=EX-521 @about=REQ-285 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 入れ子の計画
  Given 本体の既存仕様を適用する
  When KAKOI=1で存在しない名前付きプロファイルの計画を見る
  Then 計画を出さずpolicy診断となる

@id=EX-522 @about=REQ-286 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 入れ子検出の限界
  Given 本体の既存仕様を適用する
  When ホストでKAKOI=1を設定して実行する
  Then 隔離を作らず必ず入れ子警告を出す

@id=EX-523 @about=REQ-287 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 並列起動
  Given 本体の既存仕様を適用する
  When 2個の独立した起動が終了コード5と6を返す
  Then 各呼出元へそれぞれ5と6を返す

@id=EX-524 @about=REQ-288 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 診断と警告の形
  Given 本体の既存仕様を適用する
  When policy診断を出す
  Then 標準出力を空にして標準エラーに診断1行を出す

```
