# アプリ全般の公開先通知

公開対象のアプリ全般に適用する草案。kemiは利用例にすぎず専用連携は設けない。

後続の動的公開の要求。A158/A159により初版の対象から分離した。本文の初版は動的公開の初回提供を指す。初版の固定公開はnetwork/network-initial-release.mdで定義する。

- deferred: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A158, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A159

## Requirements

### REQ-138: 実際の接続先対応を通知しアプリの出力を維持する

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A136
- verification: unit

公開通知は内側とホスト側の実際のIP・ポート・TCP/UDPの対応を知らせ、アプリの出力は維持する。HTTP/HTTPS・URLのパス・認証トークンは推測せず、アプリからURLを受け取ってホスト向けURLを表示する連携は初版に含めない。URLのホスト・ポート部分を対応表に従って置き換え、パスと認証トークンを維持する使い方を示す。この契約は公開対象のアプリ全般に適用する。

## Examples

```gherkin
@id=EX-308 @about=REQ-138 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A136
Scenario: アプリの名前に依存せず接続先対応を通知する
  Given 任意のアプリの許可されたTCP待受を別番号でホスト公開した
  When 公開先を通知する
  Then 実際の内側とホスト側のIPとポートとTCPの対応を知らせる
  And 特定アプリであることを適用条件にしない

@id=EX-309 @about=REQ-138 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A136
Scenario: アプリが書いたURLを出力中で自動変更しない
  Given アプリがパスと認証トークン付きのURLを出力する
  And 公開先のホストポートは内側と異なる
  When アプリの出力と公開通知を扱う
  Then アプリの出力は維持する
  And 公開通知からHTTPかHTTPSかを推測しない

```
