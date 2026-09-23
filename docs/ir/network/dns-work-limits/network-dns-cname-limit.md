# CNAME参照の段数上限

別名を辿る段数の上限と設定を定義する草案。再試行を含む総作業量は別途定義する。

## Requirements

### REQ-119: CNAME参照の段数を設定できる

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A123
- verification: unit

CNAME参照は既定16段とし、"network.dns-max-cname-hops" で1〜128の整数に変更できる。0・無制限・小数は認めない。上位指定優先・省略時継承とし、どの段にも指定がなければ16段とする。

### REQ-120: 参照上限の超過と循環で解決を失敗させる

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A123
- verification: unit

名前から別名への1回の参照を1段と数える。上限ちょうどは許容し、さらに参照が必要なら解決失敗とする。循環を検出した場合は段数上限を待たず解決失敗とする。

## Examples

```gherkin
@id=EX-261 @about=REQ-119 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A123
Scenario: 設定の上下限を受け付ける
  Given dns-max-cname-hopsに整数1または128を指定している
  When 設定を検査する
  Then 有効な段数上限として受け付ける

@id=EX-262 @about=REQ-119 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A123
Scenario: 範囲外や整数でない段数を拒否する
  Given dns-max-cname-hopsに0または129または小数または無制限を指定している
  When 設定を検査する
  Then 設定エラーにする

@id=EX-263 @about=REQ-119 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A123
Scenario: 上位の省略では下位の値を引き継ぐ
  Given 下位で32段を指定し上位では省略している
  When 設定を合成する
  Then 段数上限は32になる

@id=EX-264 @about=REQ-120 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A123
Scenario: 上限ちょうどの参照は許容する
  Given CNAME参照の上限は16段である
  When 16段の別名参照で最終IP情報に到達する
  Then 段数上限による失敗にはしない

@id=EX-265 @about=REQ-120 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A123
Scenario: 上限を超える参照が必要なら失敗する
  Given CNAME参照の上限は16段である
  When 17段目の別名参照が必要になる
  Then 名前解決を失敗させる

@id=EX-266 @about=REQ-120 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A123
Scenario: 短い循環でも上限まで辿らない
  Given CNAME参照の上限は16段である
  When aからbを経てaへ戻る循環を検出する
  Then 上限を待たず名前解決を失敗させる
```
