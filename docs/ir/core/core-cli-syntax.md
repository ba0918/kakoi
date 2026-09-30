# コマンドラインの文法

既存仕様から継承した本体の検査用表現。同じ責務を一文書にまとめ、見出しと要求IDで参照する。この IR が正本である。

## Requirements

### REQ-250: 呼び出しの形
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

実行、計画表示、init、単独の--version、単独の--helpの5形式だけを受け付ける。実行は--とCOMMANDを必須とし、ARGSは省略でき、--以後をそのままコマンドに渡す。文法違反はusageで125。

### REQ-251: プロファイル名
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

--profileとinitのNAMEは有効なUTF-8の空でない1パス要素とし、/、0x00〜0x1F、0x7F、単独の.と..を拒否する。省略時はdefault。違反はusage。

### REQ-252: オプションの重複と値
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A10
- verification: unit

--rwと--hideだけ繰り返しを許す。他のオプションの重複はusage。値は分離形または=形で（--print-planと--nestedは=形だけ）、空を拒否し、分離形で-から始まる値は欠落としてusageにする。

### REQ-253: 計画表示の文法
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

--print-planはコマンドを実行しない。FORMはsummary、full、jsonだけを=形で受け付け、省略時summary。COMMAND省略を許すが、--を書いてCOMMANDが空ならusage。

### REQ-254: CLIのパス
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

--policy-file、--workspace、--rw、--hideの相対パスはカレントディレクトリ基準とし、~と変数を展開しない。--workspace省略時はカレントディレクトリ、--policy-fileは省略可能で指定先不在はpolicy。

### REQ-464: 入れ子の扱いの指定
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-29-nested-isolation.md#A5, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A10, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A11, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A30, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A39
- verification: unit

"--nested"はMODEとして"exec"と"isolate"だけを=形で受け付け、値の無い"--nested"と分離形はusageとする。"--nested"を書かなければ"exec"とする。実行と計画表示の形でだけ受け付け、initと単独の--versionと単独の--helpに付けるとusageで125。ポリシーには同じ指定をするキーを置かない。

## Examples

```gherkin
@id=EX-490 @about=REQ-250 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 呼び出しの形
  Given 本体の既存仕様を適用する
  When -- の後に --help を渡す
  Then コマンドの引数として保持する

@id=EX-491 @about=REQ-251 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: プロファイル名
  Given 本体の既存仕様を適用する
  When init ../x を指定する
  Then usageで125となる

@id=EX-492 @about=REQ-252 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: オプションの重複と値
  Given 本体の既存仕様を適用する
  When --rw /a --rw /b を指定する
  Then 2件を順に保持する

@id=EX-493 @about=REQ-253 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 計画表示の文法
  Given 本体の既存仕様を適用する
  When --print-plan full を指定する
  Then usageで125となる

@id=EX-494 @about=REQ-254 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: CLIのパス
  Given 本体の既存仕様を適用する
  When カレントディレクトリ/cwdから--hide ~/xを指定する
  Then /cwd/~/xとして扱う

@id=EX-896 @about=REQ-464 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A10,docs/decision/brainstorm/2026-09-29-nested-isolation.md#A39
Scenario: 入れ子の扱いの値
  Given 実行の形で起動する
  When "--nested=isolate"、"--nested isolate"、"--nested=other"、値の無い"--nested"をそれぞれ指定する
  Then 1つ目だけを受け付け、残りはusageで125になる

@id=EX-897 @about=REQ-464 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A11
Scenario: initには入れ子の扱いを付けられない
  Given initの形で起動する
  When "--nested=isolate"を付ける
  Then usageで125になる
```
