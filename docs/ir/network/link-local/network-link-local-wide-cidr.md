# 広いIPv6 CIDRとリンクローカル

リンクローカルに必要な接続口指定を広いCIDRでも維持する草案。具体的な到達経路は実測待ち。初版では提供せず後続へ分けた（未修正のpastaがリンクローカル宛て通信を運ばないため）。

## Requirements

### REQ-141: 広いIPv6 CIDRとリンクローカル

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A139
- verification: unit

IPv6の広いCIDRにリンクローカル範囲が含まれていても、接続口未指定のルールではIPv6リンクローカルを許可しない。広いCIDRの設定全体はエラーにせず、リンクローカルへの接続には "host-interface" 付きの別のIP/CIDRルールを必要とする。

## Examples

```gherkin
@id=EX-314 @about=REQ-141 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A139
Scenario: IPv6全体の指定だけではリンクローカルへ通さない
  Given 接続口指定のない::/0の許可だけがある
  When IPv6リンクローカルへ新規通信する
  Then そのルールからリンクローカルへの許可を与えない
  And ::/0の設定全体は入力エラーにしない

@id=EX-315 @about=REQ-052 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A139,docs/decision/brainstorm/2026-09-25-deferred-drafts.md#A3,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A93,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A97
Scenario: 接続口付きの別ルールでリンクローカルを許可する
  Given 接続口指定のない::/0と接続口付きリンクローカルCIDRの別ルールがある
  When 別ルールの範囲とTCP/UDPとポートに一致するリンクローカル通信を判定する
  Then 接続口付きの別ルールで許可する

```
