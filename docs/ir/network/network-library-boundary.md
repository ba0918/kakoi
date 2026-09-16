# ネットワークライブラリの責務

coreとnetの分担と、CLIを通さない利用の契約を定義する草案。

## 要求

### REQ-150: 計画と実行の責務を分ける

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A153
- 検証: review

kakoi-coreは設定の検査・合成・計画を担当し、環境へ直接アクセスしない。kakoi-netが通信の起動・監督・停止を担い、CLI以外のRustプログラムからも使える形にする。DNS応答の採否や許可期限の判断はOS操作と分けて検証できる構造にする。

## 具体例

```gherkin
@id=EX-332 @about=REQ-150 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A153
Scenario: CLI以外から通信の実行を利用できる
  Given CLI以外のRustプログラムがnetを利用する
  When 通信の起動と監督と停止を行う
  Then CLIを経由せず利用できる

@id=EX-333 @about=REQ-150 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A153
Scenario: 判断をOS操作へ混ぜない
  Given DNS応答の採否や許可期限の判断を検証する
  When 判断部分の責務を確認する
  Then OS操作と分けて検証できる
```
