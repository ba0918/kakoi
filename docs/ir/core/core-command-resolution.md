# 実行するコマンドの解決

既存仕様から継承した本体の検査用表現。同じ責務を一文書にまとめ、見出しと要求IDで参照する。この IR が正本である。

## Requirements

### REQ-260: コマンド探索
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

COMMANDに/があればそのパスの存在と実行可能性を調べる。それ以外は隔離へ渡すPATHを順に探し、PATHがなければ探索しない。見つからなければ名前を説明とするcommand not found、127。

### REQ-261: 計画と入れ子のコマンド探索
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

計画表示でCOMMAND省略時は解決を行わず該当欄を無しにする。入れ子は計画表示の有無によらず受け取ったホストPATHで探索し、PATH不在なら探索しない。

### REQ-262: exec失敗の違い
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

通常の起動でコマンドのexecが失敗すればbwrapの出力・終了コードを返す。入れ子でのexec失敗はパスとエラーを含むcommand not executableで126とする。

### REQ-263: argv0の保持
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

通常の起動と入れ子でargv[0]をCOMMANDに与えた文字列のまま渡す。解決済みパスは実行対象の指定だけに使い、bwrapには--argv0で名前を渡す。

### REQ-264: ホストでの探索の限界
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

コマンド探索はホストのファイルシステムで行い、hide配下で見つかったコマンドが後にbwrapのexecで失敗することを許容する。PATH要素がrw内かは検査しない。この隙間をREADMEへ記す。

## Examples

```gherkin
@id=EX-500 @about=REQ-260 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: コマンド探索
  Given 本体の既存仕様を適用する
  When COMMANDに存在しない/opt/toolを指定する
  Then command not foundで127となる

@id=EX-501 @about=REQ-261 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 計画と入れ子のコマンド探索
  Given 本体の既存仕様を適用する
  When 入れ子の計画表示で隔離PATHとホストPATHが異なる
  Then ホストPATHから解決したコマンドを表示する

@id=EX-502 @about=REQ-262 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: exec失敗の違い
  Given 本体の既存仕様を適用する
  When 入れ子でインタプリタのない実行可能スクリプトを起動する
  Then command not executableで126となる

@id=EX-503 @about=REQ-263 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: argv0の保持
  Given 本体の既存仕様を適用する
  When PATHから見つかったshをCOMMANDのshで起動する
  Then argv[0]は解決先の絶対パスではなくshとなる

@id=EX-504 @about=REQ-264 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: ホストでの探索の限界
  Given 本体の既存仕様を適用する
  When hide配下に実行可能コマンドが存在する
  Then 探索後のbwrap exec失敗をそのまま返す

```
