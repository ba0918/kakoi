# IPv4とIPv6の名前解決の上限単位

同じ名前に対するアドレス種別ごとの上限を定義する草案。重複問い合わせの集約、開始時点の詳細、環境全体の同時処理数と総負荷の制限は未決。

## Requirements

### REQ-124: IPv4用とIPv6用の名前解決に上限を独立して適用する

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A126
- verification: unit

同じ名前に対するIPv4用とIPv6用の名前解決には、それぞれ独立した時間・CNAME段数・上流問い合わせ回数の上限を適用する。一方の上限消費で他方の残り時間や回数を減らさない。

## Examples

```gherkin
@id=EX-278 @about=REQ-124 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A126
Scenario: IPv4の問い合わせ回数をIPv6の回数に加算しない
  Given 同じ名前のIPv4用とIPv6用の名前解決が進行している
  And それぞれの上流問い合わせ回数上限は64回である
  When IPv4用で60回とIPv6用で5回の上流問い合わせを送った
  Then 合計65回を理由に回数超過とはしない
  And IPv4用は残り4回でIPv6用は残り59回とする

@id=EX-279 @about=REQ-124 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A126
Scenario: IPv4の時間切れでIPv6の残り時間を奪わない
  Given 同じ名前のIPv4用とIPv6用の名前解決が進行している
  And IPv6用の期限まで3秒残っている
  When IPv4用の名前解決が時間上限に達する
  Then IPv6用の残り3秒は維持する

@id=EX-280 @about=REQ-124 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A126
Scenario: CNAME段数もアドレス種別ごとに判定する
  Given 同じ名前のIPv4用とIPv6用でCNAME段数上限がそれぞれ16段である
  When IPv4用が10段とIPv6用が10段を辿った
  Then 合計20段を理由に段数超過とはしない
```
