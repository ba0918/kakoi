# DNS待ち時間の設定形式

DNS待ち時間の入力範囲と設定の合成を定義する草案。

## Requirements

### REQ-117: DNS待ち時間を整数秒で指定する

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A122
- verification: unit

"network.dns-server-timeout-seconds" は既定2で1〜300秒、"network.dns-resolution-timeout-seconds" は既定10で1〜3600秒の整数とする。0・無制限・小数は認めない。

### REQ-118: 合成後の候補別期限が全体期限を超えたら拒否する

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A122
- verification: unit

DNS待ち時間の各設定は上位の指定を優先し、省略時は下位を継承する。どの段にもなければ既定値を使う。合成後に1候補の待ち時間が全体上限を超えたら設定エラーとする。

## Examples

```gherkin
@id=EX-256 @about=REQ-117 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A122
Scenario: 設定可能な上下限を受け付ける
  Given 1候補と全体の待ち時間の組が1秒と1秒または300秒と3600秒である
  When 設定を検査する
  Then 有効な待ち時間として受け付ける

@id=EX-257 @about=REQ-117 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A122
Scenario: 範囲外や整数でない待ち時間を拒否する
  Given 1候補の待ち時間が301秒または全体上限が3601秒であるかいずれかに0か無制限か小数を指定している
  When 設定を検査する
  Then 設定エラーにする

@id=EX-258 @about=REQ-118 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A122
Scenario: 上位の部分指定では残りを下位から引き継ぐ
  Given 下位で1候補5秒と全体30秒を指定している
  And 上位で全体60秒だけを指定している
  When 設定を合成する
  Then 1候補5秒と全体60秒になる

@id=EX-259 @about=REQ-118 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A122
Scenario: 合成の結果として大小関係が逆転したら拒否する
  Given 下位で1候補5秒と全体30秒を指定している
  And 上位で全体3秒だけを指定している
  When 設定を合成して検査する
  Then 1候補5秒が全体3秒を超えるため設定エラーにする

@id=EX-260 @about=REQ-118 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A122
Scenario: 指定がなければ両方の既定値を使う
  Given どの段にもDNS待ち時間の指定がない
  When 設定を合成する
  Then 1候補2秒と全体10秒になる
```
