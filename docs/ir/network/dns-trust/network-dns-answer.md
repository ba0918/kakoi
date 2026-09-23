# DNS応答と通信許可

CNAMEの参照と、許可できるIP・できないIPが混在するDNS応答の扱いを定義する草案。拒否応答はnetwork/dns-protocol/network-dns-error-response.md、処理上限はnetwork/dns-work-limits/network-dns-cname-limit.mdとnetwork/dns-work-limits/network-dns-timeout.mdで定義する。

## Requirements

### REQ-019: CNAMEの参照先の解決

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A22
- verification: unit

許可したDNS名がCNAMEで別の名前を指している場合、参照先名の追加指定なしで辿り、最終IPを検査する。許可するTCP/UDP・ポートは元のルールの範囲に限る。

### REQ-020: 混在した応答の選別

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A23, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A24
- verification: unit

DNS応答に許可できるIPと許可できないIPが混在する場合、許可できるIPだけをアプリに返して通す。別途明示許可していない内部IPは除外する。ただし署名付きの混在応答は加工せずアプリに返し、通信段階で禁止IPを拒否する。

### REQ-021: 応答の全IPが禁止の場合の拒否

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A25, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A26
- verification: unit

DNS応答で返されたIPのうち、許可できるIPが1つもない場合は、ポリシーによる拒否として名前解決を失敗させる。署名付きの応答にも適用する。IPを含まない成功応答にはしない。

## Examples

```gherkin
@id=EX-030 @about=REQ-019 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A22
Scenario: CNAMEの参照先名を追加指定せず解決する
  Given 許可したDNS名がCNAMEで別の名前を指している
  When その名前を解決する
  Then 参照先名の追加指定なしで最終IPまで辿り検査する

@id=EX-031 @about=REQ-020 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A23
Scenario: 許可可能なIPだけを残す
  Given 署名のないDNS応答に許可できる公開IPと明示許可のない内部IPが混在している
  When 応答を検査する
  Then 公開IPだけをアプリに返し内部IPは除外する

@id=EX-032 @about=REQ-020 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A24
Scenario: 署名付きの混在応答は加工しない
  Given 署名付きDNS応答に許可できる公開IPと明示許可のない内部IPが混在している
  When 応答を検査する
  Then 応答を加工せずアプリに返す

@id=EX-033 @about=REQ-020 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A24
Scenario: 応答に残した禁止IPへの通信は拒否する
  Given 署名付きの混在応答を加工せずアプリに返した
  When アプリが明示許可のない内部IPへ新規通信を試みる
  Then その通信を拒否する

@id=EX-034 @about=REQ-021 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A25
Scenario: 返されたIPがすべて禁止なら名前解決を拒否する
  Given 署名のないDNS応答で返されたIPがすべて明示許可のない内部IPである
  When 応答を検査する
  Then ポリシーによる拒否として名前解決を失敗させる
  And IPを含まない成功応答にはしない

@id=EX-035 @about=REQ-021 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A26
Scenario: 署名付きでも全IPが禁止なら名前解決を拒否する
  Given 署名付きDNS応答で返されたIPがすべて明示許可のない内部IPである
  When 応答を検査する
  Then 応答全体を拒否して名前解決を失敗させる
```
