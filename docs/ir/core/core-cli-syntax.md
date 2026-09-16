# コマンドラインの文法

既存仕様から継承した本体の検査用表現。同じ責務を一文書にまとめ、見出しと要求IDで参照する。正本は責務別spec文書群。

## 要求

### REQ-250: 呼び出しの形
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

実行、計画表示、init、単独の--version、単独の--helpの5形式だけを受け付ける。実行は--とCOMMANDを必須とし、ARGSは省略でき、--以後をそのままコマンドに渡す。文法違反はusageで125。

### REQ-251: プロファイル名
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

--profileとinitのNAMEは有効なUTF-8の空でない1パス要素とし、/、0x00〜0x1F、0x7F、単独の.と..を拒否する。省略時はdefault。違反はusage。

### REQ-252: オプションの重複と値
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

--rwと--hideだけ繰り返しを許す。他のオプションの重複はusage。値は分離形または=形で、空を拒否し、分離形で-から始まる値は欠落としてusageにする。

### REQ-253: 計画表示の文法
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

--print-planはコマンドを実行しない。FORMはsummary、full、jsonだけを=形で受け付け、省略時summary。COMMAND省略を許すが、--を書いてCOMMANDが空ならusage。

### REQ-254: CLIのパス
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

--policy-file、--workspace、--rw、--hideの相対パスはカレントディレクトリ基準とし、~と変数を展開しない。--workspace省略時はカレントディレクトリ、--policy-fileは省略可能で指定先不在はpolicy。

## 具体例

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

```
