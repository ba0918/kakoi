# 通信許可の基本判定

宛先・TCP/UDP・ポートによる通信許可の基本判定を扱う草案。DNS由来の許可の寿命は別のIRで定義する。内部アドレスの分類は未決。

## 要求

### REQ-001: 許可の一致による通過と拒否

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A4, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A15, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A36, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A38
- 検証: unit

通信許可は宛先（IP、CIDR、DNS名）・TCP/UDP・ポートの組で指定する。新規通信はいずれかの有効な許可に一致する場合に通し、どの有効な許可にも一致しない場合は拒否する。ただし隔離環境内のループバック通信はTCP/UDPの全ポートを既定で許可し、この例外によってホスト側のlocalhostには自動接続させない。また、明示的な待受公開の許可に従って、ホストのlocalhostから隔離環境内サービスへの接続を通す。

### REQ-002: DNS名による許可の識別単位

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A2
- 検証: unit

DNS名による許可は名前から得たIP・指定ポートを対象とする。許可された同じIP・ポートへの通信について、IPの直打ちか別名かによる区別はしない。

## 具体例

```gherkin
@id=EX-001 @about=REQ-001 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A4
Scenario: 指定した通信を通す
  Given 隔離環境内ループバックではない宛先のTCP・443番だけを許可している
  When その宛先のTCP・443番へ通信する
  Then 通信を通す

@id=EX-002 @about=REQ-001 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A4
Scenario: TCPの許可ではUDPを通さない
  Given 隔離環境内ループバックではない宛先のTCP・443番だけを許可している
  When その宛先のUDP・443番へ通信する
  Then 通信を拒否する

@id=EX-003 @about=REQ-001 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A4
Scenario: 指定外のポートを通さない
  Given 隔離環境内ループバックではない宛先のTCP・443番だけを許可している
  When その宛先のTCP・22番へ通信する
  Then 通信を拒否する
```
