# ホストの非ループバックIPへの接続

明示許可の方針を定義する草案。到達経路の実証は未了。

## 要求

### REQ-139: ホスト非ループバックへは通常の明示許可を使う

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A137
- 検証: unit

ホストの非ループバックIPにあるサービスへの接続は、通常の宛先と同じくIP/CIDR・TCP/UDP・ポートを明示許可して扱う。ホストという理由で自動許可しない。ホストlocalhostへの接続は既存の専用指定を維持する。

## 具体例

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

```
