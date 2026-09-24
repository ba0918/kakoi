# 上流DNSの通信方式

初版の対応方式と将来拡張の境界を定義する草案。具体API、接続先記法、証明書の信頼元、依存の選定は未決。

## Requirements

### REQ-131: 通常DNSとDNS over TLSを初版で扱う

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A131
- verification: unit

初版の明示上流は通常のDNS（UDP/TCP）とDNS over TLSに対応し、DNS over HTTPSは含めない。TLS指定時は証明書を検査し、失敗時に平文へ自動で切り替えない。

### REQ-132: DoHを追加できるよう通信方式を許可判定から分離する

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A131, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: crates/kakoi-netのDNSの通信方式を扱う部分と、DNS応答の検査・IP許可判定の部分を読み、両者が分かれていて、許可判定が通信方式に依存しないことを確かめる。

DNSの通信方式をDNS応答の検査・IP許可判定から分離し、将来DNS over HTTPSを追加できる設計とする。方式の追加で既存の許可モデルを作り直す構造にしない。

## Examples

```gherkin
@id=EX-297 @about=REQ-131 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A131
Scenario: TLS失敗で平文へ切り替えない
  Given 上流にDNS over TLSを指定している
  When サーバーの証明書を検査して失敗する
  Then その接続を採用しない
  And 平文DNSへ自動で切り替えない

@id=EX-298 @about=REQ-132 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A131
Scenario: DoHの追加箇所と共通判定を設計レビューで確認する
  Given 初版のDNS通信方式と許可判定の設計がある
  When 将来DoHを追加する変更箇所をレビューする
  Then 通信方式の追加箇所を特定できる
  And 応答の検査とIP許可判定を共通で使う構造になっている

```
