# DNSの同時処理上限

環境ごとの同時処理上限と超過時の扱いを定義する草案。キャッシュ応答の計上方法は未決。重複集約はnetwork-dns-coalescing.mdで定義する。

## 要求

### REQ-125: 環境ごとのDNS同時処理上限を設定できる

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A127
- 検証: unit

DNS同時処理上限は環境ごとに既定256件とし、"network.dns-max-concurrent-resolutions" で1〜4096の整数に変更できる。上位指定優先・省略時継承とし、どの段にも指定がなければ256件とする。0・無制限・小数は認めない。IPv4用とIPv6用は別件として数える。

### REQ-126: 処理枠を要する上限超過の問い合わせは待たせず失敗を返す

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A127
- 検証: unit

新しい処理枠が必要な問い合わせを同時処理上限の超過で受けたら、待ち行列へ入れずSERVFAILを返す。進行中の処理と環境は維持する。

## 具体例

```gherkin
@id=EX-281 @about=REQ-125 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A127
Scenario: 同時処理数の上下限を受け付ける
  Given dns-max-concurrent-resolutionsに整数1または4096を指定している
  When 設定を検査する
  Then 有効な同時処理上限として受け付ける

@id=EX-282 @about=REQ-125 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A127
Scenario: 範囲外や整数でない同時処理上限を拒否する
  Given dns-max-concurrent-resolutionsに0または4097または小数または無制限を指定している
  When 設定を検査する
  Then 設定エラーにする

@id=EX-283 @about=REQ-125 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A127
Scenario: 一方の環境の処理数で別環境の枠を減らさない
  Given 環境1と環境2はそれぞれ同時処理上限256件である
  And 環境1では256件が進行中で環境2には進行中の処理がない
  When 環境2で新規の名前解決を受ける
  Then 環境1の使用枠を理由に環境2の同時処理上限超過とはしない

@id=EX-284 @about=REQ-126 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A127
Scenario: 上限到達中の新しい処理は待たせない
  Given 環境のDNS同時処理枠が上限まで使用されている
  When 新しい処理枠を必要とする問い合わせが来る
  Then 待ち行列へ入れずSERVFAILを返す
  And 進行中の処理と環境は維持する

@id=EX-285 @about=REQ-125 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A127
Scenario: 上位で省略したら下位の同時処理上限を使う
  Given 下位で512件を指定し上位では省略している
  When 設定を合成する
  Then 同時処理上限は512件になる
```
