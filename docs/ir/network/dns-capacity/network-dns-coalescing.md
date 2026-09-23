# 重複するDNS問い合わせの集約

進行中の解決を共有する方針を定義する草案。解決条件の同一性判定の詳細は未決。応答待ち件数の上限はnetwork-dns-waiters.mdで定義する。

## Requirements

### REQ-127: 同じ解決条件の問い合わせは進行中の解決を共有する

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A128
- verification: unit

同じ隔離環境で同じ名前・種類・解決条件の問い合わせが重なったら、進行中の解決を共有し、上流の処理と同時処理枠を1件にまとめる。後から参加しても元の期限や回数上限をリセットしない。異なる環境や解決条件は混ぜず、問い合わせ元ごとの許可確認は維持する。

## Examples

```gherkin
@id=EX-286 @about=REQ-127 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A128
Scenario: 同じ条件の問い合わせは処理枠を共有する
  Given 同じ環境で許可済みの名前と種類の解決が進行中である
  When 同じ解決条件の許可された問い合わせが追加で届く
  Then 進行中の解決を共有する
  And 上流の処理と同時処理枠は1件のままとする

@id=EX-287 @about=REQ-127 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A128
Scenario: 途中参加で期限と残り回数を増やさない
  Given 共有対象の解決は期限まで2秒で上流問い合わせはあと3回送信できる
  When 同じ条件の問い合わせが途中参加する
  Then 期限まで2秒と残り3回を維持する

@id=EX-288 @about=REQ-127 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A128
Scenario: 別環境の同じ名前とは共有しない
  Given 環境1で名前の解決が進行中である
  When 環境2で同じ名前と種類の問い合わせを受ける
  Then 環境1の解決には集約しない

@id=EX-289 @about=REQ-127 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A128
Scenario: 同じ名前でも解決条件が違えば集約しない
  Given 同じ環境で名前の解決が進行中である
  When 同じ名前と種類だが解決条件が異なる問い合わせが来る
  Then その進行中の解決には集約しない

@id=EX-290 @about=REQ-127 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A128
Scenario: 集約先が存在しても問い合わせの許可確認を省略しない
  Given 共有候補となる解決が進行中である
  When 問い合わせを受ける
  Then 問い合わせ元について許可を確認してから共有の可否を判断する
```
