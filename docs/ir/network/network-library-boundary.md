# ネットワークライブラリの責務

coreとnetの分担と、CLIを通さない利用の契約を定義する草案。

## Requirements

### REQ-150: 計画と実行の責務を分ける

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A153, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4, docs/decision/brainstorm/2026-09-25-library-boundary.md#A1, docs/decision/brainstorm/2026-10-03-public-library-api.md#A15, docs/decision/brainstorm/2026-10-03-public-library-api.md#D10
- verification: review
- how_to_verify: 各クレートとCLIの構成を読み、policyの型・解析・検証・合成・ガード規則照合、planの純粋な計画、linuxのOS観測・FD準備・bwrap組立て・Landlock、netの通信判断・起動・監督・停止、runtimeの全体手順・公開API・補助役振り分け、CLIの引数・表示・従来の起動形態が分離されていることを確かめる。policyとplanがOS操作を行わず、依存がこの順を逆流しないこと、正式な入口がruntimeとpolicyに限られ、基本利用がruntimeだけで完結すること、DNS判断がnet内でOS操作と分離されていることを確かめる。

policyはポリシーの型・解析・検証・合成とガード規則の照合を担当し、planは渡された事実からの純粋な計画を担当する。
policyとplanはOS操作を行わない。
linuxはOS観測、ファイル記述子の準備、bwrapの組立て、Landlockの適用を担当する。
netは通信の判断と起動・監督・停止を担当し、DNS応答の採否と許可期限の判断をnet内でOS操作から分離する。
runtimeは全体の手順、公開API、補助役の振り分けを担当し、CLIは引数の解釈、表示、従来の起動形態を担当する。
依存はpolicy、plan、linux、net、runtime、CLIの順を逆流しない。
正式な利用者向け入口はruntimeとpolicyに限り、基本利用はruntimeへの依存追加だけで必要なポリシー型も利用できる。

## Examples

```gherkin
@id=EX-332 @about=REQ-150 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A153,docs/decision/brainstorm/2026-10-03-public-library-api.md#A15,docs/decision/brainstorm/2026-10-03-public-library-api.md#D10
Scenario: CLI以外から通信の実行を利用できる
  Given CLI以外のRustプログラムがruntimeを利用する
  When 通信の起動と監督と停止を行う
  Then CLIを経由せず利用できる

@id=EX-333 @about=REQ-150 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A153
Scenario: 判断をOS操作へ混ぜない
  Given DNS応答の採否や許可期限の判断を検証する
  When 判断部分の責務を確認する
  Then OS操作と分けて検証できる
```
