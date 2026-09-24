# 診断と終了結果

既存仕様から継承した本体の検査用表現。同じ責務を一文書にまとめ、見出しと要求IDで参照する。この IR が正本である。

## Requirements

### REQ-289: 制御文字の表示
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

kakoi自身の出力へ埋め込む値の0x00〜0x1Fと0x7Fを見える表記に逃がす。診断と警告は末尾LF以外の改行を持たない。U+2028、U+2029、U+0085は対象外。コマンド出力は変更しない。JSON以外の不正UTF-8とエスケープ表記の選択は委譲する。

### REQ-290: 診断の終了コード
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

usage、policy、path、secret、env、bwrapは125、command not foundは127、入れ子のcommand not executableは126で終わり標準出力を出さない。診断条件の詳細は各責務の検査規則に従う。

### REQ-291: コマンドとbwrapの終了
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

host/noneではbwrapへexecし、bwrap自身の失敗出力・終了コードをそのまま返す。主コマンドの標準出力・標準エラーと終了コードnを返し、シグナルsの終了は128+sとする。filteredの監督と安全上の125優先は既存のネットワーク仕様を優先する。

### REQ-292: 計画の事前検査
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

計画表示はポリシー・事実・秘密を読み、bwrapの所在確認と指定COMMANDの解決まで行う。診断があれば計画を出さずその診断で終わり、成功時は計画を標準出力へ出して0とする。

### REQ-293: 検査の段階
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

検査はhelp/version、文法、cwd、HOME、ポリシー読込合成、workspaceと変数、マウント解決、bwrap所在、コマンド解決、起動の順で最初の診断に止まる。initは文法後HOMEと書込みのみ、計画なし入れ子は文法後に警告してコマンド解決へ進む。

## Examples

```gherkin
@id=EX-525 @about=REQ-289 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 制御文字の表示
  Given 本体の既存仕様を適用する
  When 秘密名にESCが含まれる警告を出す
  Then 生ESCを出さず警告を1行に保つ

@id=EX-526 @about=REQ-290 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 診断の終了コード
  Given 本体の既存仕様を適用する
  When コマンドが見つからない
  Then command not foundで127となる

@id=EX-527 @about=REQ-291 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: コマンドとbwrapの終了
  Given 本体の既存仕様を適用する
  When 主コマンドがSIGTERMで終了するhost起動
  Then 終了コード143を返す

@id=EX-528 @about=REQ-292 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 計画の事前検査
  Given 本体の既存仕様を適用する
  When 計画表示時にポリシーが不正である
  Then 計画を出さずpolicyで125となる

@id=EX-529 @about=REQ-293 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 検査の段階
  Given 本体の既存仕様を適用する
  When HOME不正とポリシー不正が同時に成立する
  Then 先のenv診断を出す

```
