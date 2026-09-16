# DNS解決の境界

CNAME解決の失敗、無関係なIPの扱い、DNSSEC検証の担当を定義する草案。上限値と設定はnetwork/dns-work-limits/network-dns-cname-limit.md、network/dns-work-limits/network-dns-timeout.md、network/dns-work-limits/network-dns-query-limit.mdで定義する。

## 要求

### REQ-023: 完了できないCNAME解決の失敗

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A28
- 検証: unit

CNAMEの循環、参照回数または解決時間の上限超過時は名前解決を失敗させ、その不完全な結果から新しいIP許可を作らない。

### REQ-024: 無関係な追加IPの許可除外

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A29
- 検証: unit

問い合わせた許可名から辿れる最終IPだけを許可候補とする。同じDNS応答に同梱された無関係な名前のIPは、その応答を根拠とした許可に加えない。

### REQ-025: DNSSEC検証の担当

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A30
- 検証: review

DNSSECの署名検証は設定された問い合わせ先DNSサーバーに任せ、kakoi-net自身は暗号学的な署名検証を行わない。kakoi-netはIP・ポートの許可判定を担当する。DNSSECの保証は上流の検証設定と通信経路に依存し、非検証の上流を使う場合には保証しない。

## 具体例

```gherkin
@id=EX-038 @about=REQ-023 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A28
Scenario: CNAMEが循環したら解決を失敗させる
  Given CNAMEが名前Aから名前Bを指し名前Bから名前Aを指している
  When 名前Aの解決で循環を検出する
  Then 名前解決を失敗させる
  And その不完全な結果から新しいIP許可を作らない

@id=EX-039 @about=REQ-023 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A28
Scenario: 参照回数の上限を超えたら解決を失敗させる
  Given CNAMEの解決を進めている
  When 参照回数の上限を超える
  Then 名前解決を失敗させる
  And その不完全な結果から新しいIP許可を作らない

@id=EX-040 @about=REQ-023 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A28
Scenario: 解決時間の上限を超えたら解決を失敗させる
  Given CNAMEの解決を進めている
  When 解決時間の上限を超える
  Then 名前解決を失敗させる
  And その不完全な結果から新しいIP許可を作らない

@id=EX-041 @about=REQ-024 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A29
Scenario: 同梱された無関係なIPを許可に加えない
  Given 許可名のDNS応答に参照経路と無関係な名前のIPが同梱されている
  When その応答から許可候補を選ぶ
  Then 無関係な名前のIPをその応答を根拠とした許可に加えない
```
