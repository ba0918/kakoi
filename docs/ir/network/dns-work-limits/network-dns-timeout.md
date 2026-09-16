# DNS問い合わせの待ち時間

待ち時間の既定値と変更可否を定義する草案。設定形式と範囲はnetwork/dns-upstream-config/network-dns-timeout-config.mdで定義する。通信方式ごとの時間計測の詳細は未決。既定値は製品の選択でありRFCの指定値や実測済み性能ではない。

## 要求

### REQ-116: 候補別と名前解決全体の期限を設定できる

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A121
- 検証: unit

明示した上流DNSの1候補を待つ時間は既定2秒、名前解決全体の上限は既定10秒とし、どちらも設定で変更できる。別候補への切替、CNAME参照、ホストDNS設定変更によるやり直しで全体の期限を延長しない。ホストDNS利用時も全体上限を適用するが、ホスト内部の候補選択に1候補上限を直接適用する規則ではない。

## 具体例

```gherkin
@id=EX-252 @about=REQ-116 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A121
Scenario: 既定では1候補を2秒待つ
  Given DNS待ち時間の設定を省略している
  And 明示上流の次の候補があり全体の期限にも余裕がある
  When 現在の候補が2秒間応答しない
  Then その候補の待機を打ち切り次の候補へ進む

@id=EX-253 @about=REQ-116 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A121
Scenario: やり直しを含め全体は既定10秒で打ち切る
  Given DNS待ち時間の設定を省略している
  And 別候補への切替とCNAME参照と設定変更によるやり直しが発生した
  When 名前解決処理が未完了のまま開始から10秒に達する
  Then 全体の期限切れとして処理を打ち切る

@id=EX-254 @about=REQ-116 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A121
Scenario: 遅い環境に合わせて待ち時間を変更できる
  Given 1候補の待ち時間を5秒で全体上限を30秒に設定した
  When 明示上流への名前解決を開始する
  Then 1候補5秒と全体30秒の上限を使う

@id=EX-255 @about=REQ-116 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A121
Scenario: ホストDNSでも全体上限を適用する
  Given ホストのDNS設定を使い全体上限の設定は省略している
  When 名前解決処理が未完了のまま開始から10秒に達する
  Then 全体の期限切れとして処理を打ち切る
```
