# UDPの自動公開

UDP公開の対応範囲と対象を定義する草案。ソケットの検出と再利用、相手別の中継状態は未決。

後続の動的公開の要求。A158/A159により初版の対象から分離した。本文の初版は動的公開の初回提供を指す。初版の固定公開はnetwork/network-initial-release.mdで定義する。

## Requirements

### REQ-040: UDP公開の初版対応

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A45
- verification: unit

初版からUDP公開を扱う。公開先はホストのlocalhostに限定し、TCPの公開許可だけでUDPを公開しない。

### REQ-041: 許可範囲内のUDPソケットの自動公開

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A45
- verification: unit

事前にUDP公開を許可した範囲内で、ローカルのアドレスとポートを割り当てられたソケットを検出したら、自動公開の対象とする。アプリ名や用途では区別せず、外向き通信の送信元ポートも範囲内なら対象とする。

### REQ-042: UDPソケットの寿命に連動する公開

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A46
- verification: unit

対象UDPソケットが開いている間は無通信でも公開を維持し、ソケットが閉じられたことを検出したら公開を止める。外向きUDP通信の無通信期限を公開自体の停止条件にしない。

## Examples

```gherkin
@id=EX-071 @about=REQ-040 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A45
Scenario: 明示許可したUDPサービスを公開する
  Given UDP公開を許可した範囲内でUDPサービスが受信可能なソケットを開いている
  And ホスト側の公開ポートを確保できる
  When そのソケットを公開する
  Then ホストのlocalhostの公開先からUDPサービスへデータを送れる

@id=EX-072 @about=REQ-040 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A45
Scenario: TCP公開の許可をUDPに適用しない
  Given あるポートはTCP公開だけを許可されている
  When その番号のUDPソケットを検出する
  Then そのTCP公開許可を根拠にUDPソケットを公開しない

@id=EX-073 @about=REQ-041 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A45
Scenario: 外向き通信の送信元でも公開許可範囲内なら対象になる
  Given 外向き通信の送信元として使うUDPソケットのローカルポートがUDP公開許可範囲内にある
  When そのソケットを検出する
  Then 用途を理由に除外せず自動公開の対象とする

@id=EX-074 @about=REQ-041 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A45
Scenario: UDP公開許可範囲外のソケットは公開しない
  Given UDPソケットのローカルポートがUDP公開許可範囲外にある
  When そのソケットを検出する
  Then 自動公開の対象にしない

@id=EX-075 @about=REQ-042 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A46
Scenario: 通信がなくても開いているUDPソケットの公開を維持する
  Given 公開対象のUDPソケットが開いたままである
  When 外向きUDP通信の無通信期限を超える時間通信がない
  Then UDP公開を維持する

@id=EX-076 @about=REQ-042 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A46
Scenario: UDPソケットの終了を検出したら公開を止める
  Given UDPソケットを公開している
  When 対象ソケットが閉じられたことを検出する
  Then そのソケットの公開を止める
```
