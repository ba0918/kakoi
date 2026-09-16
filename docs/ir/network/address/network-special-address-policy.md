# 特殊用途IPの明示許可

既存の個別規則以外の特殊用途範囲を定義する草案。参照分類表の版と使用不能宛先の細則は未決。

## 要求

### REQ-140: 特殊用途IPの明示許可

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A138
- 検証: unit

既存の個別規則で扱っていないIANA特殊用途IPはDNS名の許可だけでは通さず、IP/CIDR・TCP/UDP・ポートの明示許可を必要とする。登録された一部の公開サービス用宛先も対象とする。既存のループバック規則と対象外通信の扱いは維持する。

## 具体例

```gherkin
@id=EX-312 @about=REQ-140 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A138
Scenario: DNS名の許可だけでは特殊用途IPを許可しない
  Given 許可名の解決先が個別規則で扱っていない特殊用途IPである
  And そのIPへの明示許可はない
  When DNS応答から通信許可を判定する
  Then その特殊用途IPを許可に加えない

@id=EX-313 @about=REQ-140 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A138
Scenario: 特殊用途IPへの明示許可を条件どおり扱う
  Given 対応範囲内の特殊用途IPとTCP443を明示許可している
  When その宛先のTCP443への通信を判定する
  Then 特殊用途IPであることだけを理由に拒否しない

```
