# UDP無通信期限の設定範囲

UDP無通信期限の入力上限とレイヤ間の合成を定義する草案。

## Requirements

### REQ-093: UDP無通信期限の最大値

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A98
- verification: unit

UDP無通信期限の設定可能範囲は1〜86400秒とし、既定120秒を維持する。これは最後の通信から待つ時間の上限であり、通信が続くフローの総時間を1日に制限するものではない。

### REQ-094: UDP無通信期限の上位レイヤ優先

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A99
- verification: unit

UDP無通信期限は上の段で指定した値が上書きする。省略なら下の段の値を引き継ぎ、どこにも指定がなければ120秒とする。

## Examples

```gherkin
@id=EX-198 @about=REQ-093 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A98
Scenario: 無通信期限の上限値を受け付ける
  Given network.udp-idle-timeout-secondsに86400を指定している
  When 値を検査する
  Then 有効な無通信期限として受け付ける

@id=EX-199 @about=REQ-093 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A98
Scenario: 無通信期限の上限超過を拒否する
  Given network.udp-idle-timeout-secondsに86401を指定している
  When 値を検査する
  Then 設定エラーにする

@id=EX-200 @about=REQ-093 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A98
Scenario: 通信が続くフローを開始から1日で打ち切らない
  Given UDP無通信期限を86400秒に設定している
  And 同じフローで無通信期限を超えずに許可された送受信が続いている
  When フロー開始から1日を超える
  Then 総時間が1日を超えたことだけを理由にそのフローを打ち切らない

@id=EX-201 @about=REQ-094 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A99
Scenario: 上の段で期限を延ばせる
  Given 下の段のUDP無通信期限が120秒で上の段が600秒である
  When 設定を合成する
  Then UDP無通信期限は600秒になる

@id=EX-202 @about=REQ-094 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A99
Scenario: 上の段の省略では下の段を維持する
  Given 下の段のUDP無通信期限が600秒で上の段には指定がない
  When 設定を合成する
  Then UDP無通信期限は600秒になる

@id=EX-203 @about=REQ-094 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A99
Scenario: どの段にもなければ既定値を使う
  Given どの段にもUDP無通信期限の指定がない
  When 設定を合成する
  Then UDP無通信期限は120秒になる
```
