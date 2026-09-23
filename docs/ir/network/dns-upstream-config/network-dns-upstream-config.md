# 明示上流の記法と合成

今回合意した追加・改訂部分の草案。方式の実証は別途必要。

## Requirements

### REQ-146: 明示上流の記法と合成

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A144
- verification: unit

明示DNS上流は "network.dns-upstream" 配列で指定し、下位から上位へ連結する。接続IPとTLSの証明書照合名を分ける。上流指定がなければホストDNSを使う。同じ候補一覧に平文とTLSを混在させない。空配列では下位を消さず、総入替えには別プロファイルを使う。

## Examples

```gherkin
@id=EX-324 @about=REQ-146 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A144
Scenario: 上位の空配列は下位の上流を消さない
  Given 下位に明示上流があり上位の上流配列は空である
  When 設定を合成する
  Then 下位の上流は候補に残る

@id=EX-325 @about=REQ-146 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A144
Scenario: 異なる保護方式の混在を受け付けない
  Given 下位にTLS上流があり上位に平文上流がある
  When 設定を合成する
  Then 混在した候補一覧を受け付けない

```
