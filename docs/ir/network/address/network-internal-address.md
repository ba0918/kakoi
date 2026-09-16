# 内部アドレスの明示許可

DNS名の許可だけでは到達を認めない内部範囲を定義する草案。その他の特殊用途アドレスとリンクローカルの接続経路は未決。

## 要求

### REQ-043: IPまたはCIDRの明示許可を要する内部範囲

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A47, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A5
- 検証: unit

次の範囲はDNS名の許可だけでは通さず、対象IPまたはCIDRとTCP/UDP・ポートの明示許可を要求する。

IPv4プライベートは10.0.0.0/8、172.16.0.0/12、192.168.0.0/16
IPv6内部利用向けはfc00::/7
共有アドレスは100.64.0.0/10
リンクローカルは169.254.0.0/16、fe80::/10

隔離環境内のlocalhostは既存のループバック許可の例外に従う。

## 具体例

```gherkin
@id=EX-077 @about=REQ-043 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A47
Scenario: DNS許可だけでは共有アドレスへ通さない
  Given 許可したDNS名が100.64.0.1に解決された
  And そのIPを含む明示的なIPまたはCIDRの許可がない
  When そのIPへ新規通信を試みる
  Then 通信を拒否する

@id=EX-078 @about=REQ-043 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A47
Scenario: DNS許可だけではIPv6内部アドレスへ通さない
  Given 許可したDNS名がfd00::1に解決された
  And そのIPを含む明示的なIPまたはCIDRの許可がない
  When そのIPへ新規通信を試みる
  Then 通信を拒否する

@id=EX-079 @about=REQ-043 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A47
Scenario: 明示許可した内部IPへの通信を通す
  Given 192.168.1.10のTCP・443番への明示許可と通信経路がある
  When そのIPのTCP・443番へ新規通信する
  Then 通信を通す
```
