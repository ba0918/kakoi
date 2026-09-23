# filteredの監督プロセス

今回合意した追加・改訂部分の草案。方式の実証は別途必要。

## Requirements

### REQ-148: filteredの監督プロセス

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A146
- verification: unit

"filtered" では監督プロセスを残し、通信障害からの復帰と主コマンド終了後の子プロセス回収を担う。bwrapへのexec契約を "filtered" に限り改訂し、"host" と "none" は維持する。

## Examples

```gherkin
@id=EX-328 @about=REQ-148 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A146
Scenario: filteredでは監督を残す
  Given filteredで環境を起動する
  When 主コマンドが動作している
  Then 監督プロセスが残る

@id=EX-329 @about=REQ-148 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A146
Scenario: 従来のモードの実行方式を変えない
  Given hostまたはnoneで環境を起動する
  When bwrapへ実行を移す
  Then 従来のexec契約を維持する

```
