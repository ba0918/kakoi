# DNSの問い合わせ先と対象

既定の問い合わせ先と変更可否、問い合わせ対象を定義する草案。名前別振り分けも引き継ぐ。ホスト設定の取得方法と対応環境、指定形式、設定変更の検知方式と遅延は未決。変更追従の方針はnetwork/dns-host-settings/network-dns-settings-change.mdで定義する。

## Requirements

### REQ-026: 上流DNSの既定と変更

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A31
- verification: unit

上流DNSは既定でホストのDNS設定を利用し、ポリシーで問い合わせ先を変更できるようにする。

### REQ-027: 管理するDNS経路の問い合わせ対象

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A32
- verification: unit

kakoi-netが管理するDNS経路では、許可ルールに一致する名前の問い合わせだけを上流に送り、それ以外は上流へ送らず拒否する。許可名からCNAMEで辿る参照先は、追加の名前指定なしで解決する。

### REQ-106: ホストDNSの名前別振り分けを引き継ぐ

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A111
- verification: unit

上流DNSを明示指定せずホストのDNS設定を利用する場合、名前ごとに問い合わせ先を分けるホストの設定も初版から引き継ぐ。管理するDNS経路で問い合わせを許可する名前の境界は維持する。

## Examples

```gherkin
@id=EX-042 @about=REQ-026 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A31
Scenario: 問い合わせ先の指定がなければホストのDNS設定を使う
  Given ポリシーで問い合わせ先DNSサーバーを指定していない
  When 上流DNSを選ぶ
  Then ホストのDNS設定を利用する

@id=EX-043 @about=REQ-026 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A31
Scenario: ポリシーで問い合わせ先を変更する
  Given ポリシーで問い合わせ先DNSサーバーを明示指定した
  When 上流DNSを選ぶ
  Then ポリシーで指定した問い合わせ先を使う

@id=EX-044 @about=REQ-027 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A32
Scenario: 許可されていない名前の問い合わせを上流へ送らない
  Given 許可した名前は "api.example.com" だけである
  When 管理するDNS経路に "other.example.net" の問い合わせが来る
  Then 上流へ送らず拒否する

@id=EX-045 @about=REQ-027 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A32
Scenario: 許可名からの別名参照は解決する
  Given 許可名 "api.example.com" のCNAMEが "edge.example.net" を指している
  When 許可名の解決を続けるため参照先を問い合わせる
  Then 参照先名の追加許可なしで問い合わせを認める

@id=EX-046 @about=REQ-027 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A32
Scenario: 許可名の問い合わせを上流へ送る
  Given "api.example.com" が許可ルールに一致する
  When 管理するDNS経路にその名前の問い合わせが来て上流への問い合わせが必要になる
  Then 上流への問い合わせを認める
@id=EX-230 @about=REQ-106 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A111
Scenario: 許可した社内名と一般名をホストの設定に沿って問い合わせる
  Given 上流DNSを明示指定していない
  And ホストでは社内名を社内DNSへ一般名を別のDNSへ振り分けている
  And 問い合わせる社内名と一般名はどちらも許可済みである
  When それぞれの名前について上流問い合わせを開始する
  Then ホストの設定に従いそれぞれの問い合わせ先へ送る

@id=EX-231 @about=REQ-106 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A111
Scenario: ホストに問い合わせ先があっても未許可の名前は送らない
  Given ホストに社内名の問い合わせ先が設定されている
  And 対象の社内名は許可ルールに一致しない
  And 許可名からのCNAME参照先でもない
  When 管理するDNS経路にその名前の問い合わせが来る
  Then 上流へ送らず拒否する
```
