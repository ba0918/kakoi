# ホストの非ループバックIPへの接続

明示許可の方針を定義する草案。到達経路の実証は未了。

## Requirements

### REQ-139: ホスト非ループバックへは通常の明示許可を使う

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A137
- verification: unit

ホストの非ループバックIPにあるサービスへの接続は、通常の宛先と同じくIP/CIDR・TCP/UDP・ポートを明示許可して扱う。ホストという理由で自動許可しない。ホストlocalhostへの接続は既存の専用指定を維持する。

### REQ-425: ホスト自身のアドレスは起動時に一度だけ読む

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A7, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A137
- verification: unit

"filtered" は、ホスト自身が持つアドレスを起動時に一度だけ読み、REQ-139のホストの非ループバックIPとして扱う。起動後にホストが得たアドレスはホストのアドレスとして扱わず、他の宛先と同じく "dns" の許可だけで開きうる。

## Examples

```gherkin
@id=EX-310 @about=REQ-139 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A137
Scenario: 明示許可したホストのLAN側サービスに接続する
  Given ホストのLAN側IPとTCP5432を明示許可しサービスが待ち受けている
  And 接続経路が利用可能である
  When アプリがそのIPとポートへ接続する
  Then 通信を許可する

@id=EX-311 @about=REQ-139 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A137
Scenario: ホストのIPという理由だけで許可しない
  Given ホストの非ループバックIPへの有効な通信許可はない
  When アプリがそのIPへ新規通信を試みる
  Then ホストのIPという理由だけで許可しない

@id=EX-816 @about=REQ-425 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A7,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A137
Scenario: 起動時にホストが持つアドレスはDNSの許可だけでは開かない
  Given 起動時にホストが特別用途でない公開アドレスを持ち、"dns" の許可の名前がそのアドレスに解決される
  When アプリがその名前を解決して許可したポートでそのアドレスへ新規通信を試みる
  Then その通信を許可しない

@id=EX-817 @about=REQ-425 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A7,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A137
Scenario: 起動後にホストが得たアドレスはDNSの許可で開く
  Given 起動後にホストがVPNの接続で特別用途でない公開アドレスを得て、"dns" の許可の名前がそのアドレスに解決される
  When アプリがその名前を解決して許可したポートでそのアドレスへ新規通信を試みる
  Then その通信を許可する
```
