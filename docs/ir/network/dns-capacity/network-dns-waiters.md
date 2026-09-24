# 共有する名前解決の応答待ち上限

応答先として保持する問い合わせ件数を定義する草案。同一要求の再送識別と保持サイズの詳細は未決。

## Requirements

### REQ-128: 解決ごとの応答待ち件数を設定できる

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A129
- verification: unit

共有する解決1件あたりの応答待ちは既定64件とし、"network.dns-max-waiters-per-resolution" で1〜1024の整数に変更できる。上の段の指定を優先し、省略時は下の段の値を使い、どの段にも指定がなければ64件とする。最初の問い合わせも含め、個別に応答を返す問い合わせの数で数える。アプリ数では数えない。

### REQ-129: 応答待ちの上限を超える新しい問い合わせを拒む

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A129
- verification: unit

応答待ち件数を超過する新しい問い合わせにはSERVFAILを返し、既存の応答待ちと解決処理は維持する。別の解決を作って上限を迂回しない。

## Examples

```gherkin
@id=EX-291 @about=REQ-128 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A129
Scenario: 最初の問い合わせも応答待ちに数える
  Given 応答待ち上限は64件である
  When 最初の問い合わせと63件の追加問い合わせを同じ解決で待たせている
  Then 応答待ちは64件として数える

@id=EX-292 @about=REQ-128 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A129
Scenario: 同じアプリでも個別の応答先は別件で数える
  Given 同じアプリから個別に応答を返す2件の問い合わせが同じ解決を共有している
  When 応答待ち件数を数える
  Then 2件と数える

@id=EX-293 @about=REQ-129 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A129
Scenario: 上限超過分を別の解決へ逃がさない
  Given 共有する解決の応答待ちは上限64件に達している
  When 同じ解決を共有する新しい問い合わせが来る
  Then 新しい問い合わせにSERVFAILを返す
  And その問い合わせ用に別の解決を作らない
  And 既存の64件の応答待ちと解決処理は維持する

@id=EX-294 @about=REQ-128 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A129
Scenario: 上位で省略した応答待ち上限は下位から引き継ぐ
  Given 下位で応答待ち上限128件を指定し上位では省略している
  When 設定を合成する
  Then 応答待ち上限は128件になる
```
