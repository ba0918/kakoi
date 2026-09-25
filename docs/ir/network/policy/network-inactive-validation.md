# 使用しない公開設定の競合検査

host・noneで使用しない公開設定同士の検査を定義する草案。

後続の動的公開の入力検査。初版の固定公開の入力検査はnetwork/network-initial-release.mdで定義する。

- deferred: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A158, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A159

## Requirements

### REQ-086: 非使用の公開設定同士の競合拒否

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A91
- verification: unit

"host"・"none" で使わない公開設定同士の競合も検査し、競合があれば起動前エラーにする。使わない設定のDNS問い合わせ・インターフェース存在確認は引き続き省略する。

## Examples

```gherkin
@id=EX-177 @about=REQ-086 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A91
Scenario: hostでも公開先が競合する設定は拒否する
  Given modeにhostを明示している
  And 同じTCP8000番の公開先を18000番と19000番に指定した2つの公開設定がある
  When 起動前に設定を検査する
  Then 公開設定の競合を設定エラーにしてアプリを実行しない

@id=EX-178 @about=REQ-086 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A91
Scenario: noneでも公開系統が競合する設定は拒否する
  Given modeにnoneを明示している
  And 同じTCP8000番と同じホスト許可範囲の2つの公開設定がある
  And 一方のhost-familyはipv4でもう一方はipv6である
  When 起動前に設定を検査する
  Then 公開設定の競合を設定エラーにしてアプリを実行しない
```
