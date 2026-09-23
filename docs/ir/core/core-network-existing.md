# 既存のhostとnone

追加のfiltered契約とは分け、以前から提供する二つのモードの動作を抽出する。

## Requirements

### REQ-388: 従来のhostとnoneの通信範囲
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

hostはホストのネットワークをそのまま使う。noneはネットワーク名前空間を分離し、ループバックだけを残す。noneから外部アドレスへの接続は失敗する。

## Examples

```gherkin
@id=EX-716 @about=REQ-388 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: noneでは外部へ経路を持たない
  Given noneで起動した隔離内のプログラムがある
  When 192.0.2.1への接続を試みる
  Then 外部への経路がなく接続できない

@id=EX-717 @about=REQ-388 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: hostではホストの経路を使う
  Given hostで起動した隔離内のプログラムがある
  When ネットワーク経路を使う
  Then ホストと同じネットワークで動作する
```
