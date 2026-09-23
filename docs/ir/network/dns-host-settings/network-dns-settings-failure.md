# 変更後のホストDNS設定取得失敗

実行中の設定変更を検知した後の取得・解釈失敗を定義する草案。設定取得の回復検知方法と具体的なDNS失敗応答は未決。単なる上流DNSの応答タイムアウトとは区別する。

## Requirements

### REQ-110: 新しいホストDNS設定が使えない間は別の設定で問い合わせない

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A115
- verification: unit

ホストDNS設定の変更を検知しても新しい設定を取得・解釈できない間は、上流への問い合わせを失敗として扱い、旧設定や別のDNSへ自動で切り替えない。環境全体は終了せず、既存IP許可と継続中の通信には既存規則を適用する。

## Examples

```gherkin
@id=EX-239 @about=REQ-110 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A115
Scenario: 変更後の設定を読めなくても古い問い合わせ先へ送らない
  Given ホストDNS設定の変更を検知したが新しい設定を取得できない
  When 許可名について上流への問い合わせが必要になる
  Then 問い合わせを失敗として扱う
  And 旧設定や別のDNSへ自動で問い合わせない

@id=EX-240 @about=REQ-110 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A115
Scenario: 新しい設定を解釈できない場合も問い合わせ先を代替しない
  Given ホストDNS設定の変更を検知し新しい設定を取得したが解釈できない
  When 許可名について上流への問い合わせが必要になる
  Then 問い合わせを失敗として扱う
  And 旧設定や別のDNSへ自動で問い合わせない

@id=EX-241 @about=REQ-110 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A115
Scenario: DNS設定取得失敗だけでは環境や継続中の通信を終了しない
  Given 既存のIP許可と継続中のTCPとUDPの通信がある
  When 変更後のホストDNS設定を取得できなくなる
  Then 環境全体を終了しない
  And 既存IP許可は元の期限を維持する
  And 継続中のTCPとUDPは既存の継続規則で扱う
```
