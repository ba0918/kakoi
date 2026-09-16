# 安全上の故障時の終了結果優先

競合する終了結果の優先順位を定義する草案。故障検知と終了順序の実証は未了。

## 要求

### REQ-142: 安全上の故障時の終了結果優先

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A140
- 検証: unit

通信制限と遮断の双方を保証できない安全上の故障で環境を終了する場合、主コマンドの成功・失敗やSIGTERMの143よりkakoiの終了コード125を優先し原因を通知する。通信制限を維持できている通常のDNS失敗にはこの優先規則を適用しない。

## 具体例

```gherkin
@id=EX-316 @about=REQ-142 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A140
Scenario: SIGTERM終了中でも安全上の故障を優先する
  Given SIGTERMによる終了結果143で終了する処理が進行中である
  When 通信制限と遮断の双方を保証できない故障により環境を終了する
  Then kakoiの終了コードは125になる
  And 故障原因を通知する

@id=EX-317 @about=REQ-142 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A140
Scenario: DNS失敗だけでは正常終了を故障結果に置き換えない
  Given 名前解決は失敗したが通信制限は維持できている
  When 主コマンドが0で終了する
  Then DNS失敗だけを理由に終了コードを125へ置き換えない

```
