# 通知の保持上限と現在の状態の優先

保持量と出力回復時の方針を定義する草案。書込みの隔離機構と1件の最大長は未決。

## 要求

### REQ-144: 通知の保持上限と現在の状態の優先

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A142
- 検証: unit

通知の保持量は最大256件かつ合計1MiBまでとする。出力先が詰まったら古い通知から捨て、出力回復後は欠落があったことと現在の状態を優先する。通知待ちで通信制御を止めない。

## 具体例

```gherkin
@id=EX-320 @about=REQ-144 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A142
Scenario: 保持上限を超える前に古い通知を捨てる
  Given 通知の出力先が詰まり通知を保持している
  When 新しい通知を保持すると256件または1MiBの上限を超える
  Then 古い通知から捨てて両方の上限内に保つ
  And 通信制御を通知待ちで止めない

@id=EX-321 @about=REQ-144 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A142
Scenario: 出力回復後は欠落と現在の状態を優先する
  Given 出力先の詰まりにより古い通知を捨てた
  When 出力先が回復する
  Then 欠落があったことと現在の状態を優先して知らせる

```
