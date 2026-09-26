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
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-command-policy.md#A10, docs/decision/brainstorm/2026-09-25-command-policy.md#A37, docs/decision/brainstorm/2026-09-25-command-policy.md#A40
- verification: unit

usage、policy、path、secret、env、bwrapは125、command not foundは127、入れ子と見張り役のcommand not executableと見張り役のguardは126で終わり標準出力を出さない。診断条件の詳細は各責務の検査規則に従う。

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

### REQ-400: カレントディレクトリの取得の失敗
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A10
- verification: unit

REQ-293 のcwdの段階でカレントディレクトリを取得できないとき（削除されたディレクトリの中から起動した場合を含む）、種類pathの診断を出して125で終わる。

### REQ-401: bwrap自身のexecの失敗
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A11, docs/decision/brainstorm/2026-09-25-command-policy.md#A40
- verification: unit

host/noneの起動の段階でbwrap自身のexecに失敗したとき（所在確認の後に消された、実行できない）は、まだkakoiが動いているので種類bwrapの診断を出して125で終わる。包んだコマンドのexecの失敗はbwrapが報告し、REQ-291のとおりbwrapの失敗出力と終了コードをそのまま返す。入れ子ではkakoiがコマンドを直接execするので、同じ失敗がREQ-290のcommand not executableの126になる。見張り役が本物をexecするときも同じである。この非対称は受け入れる。

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

@id=EX-753 @about=REQ-400 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A10,docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 削除されたカレントディレクトリ
  Given カレントディレクトリが削除されている
  When kakoi -- trueを実行する
  Then 標準出力を出さずpathの診断で125となる

@id=EX-754 @about=REQ-400 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A10,docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 取得できるカレントディレクトリ
  Given カレントディレクトリは取得できHOMEが相対パスである
  When kakoi -- trueを実行する
  Then pathではなく次の段階のenvの診断で止まる

@id=EX-755 @about=REQ-401 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A11
Scenario: 所在確認の後に実行できないbwrap
  Given PATH上のbwrapが存在しないインタプリタを指す実行可能なスクリプトである
  When host起動でbwrapをexecする
  Then bwrapの診断で125となる

@id=EX-756 @about=REQ-401 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A11
Scenario: 包んだコマンドのexecの失敗
  Given 包むコマンドが隠された場所にある
  When host起動で包んだコマンドのexecが失敗する
  Then kakoiの診断を出さずbwrapの失敗出力と終了コードを返す

```
