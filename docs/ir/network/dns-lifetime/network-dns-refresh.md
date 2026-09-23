# DNS情報の更新契機

問い合わせに応じて更新する方針を定義する草案。TTLゼロの有限猶予はnetwork-dns-lifetime.mdで定義し、安全な実行機構は実証で確認する。

## Requirements

### REQ-133: 問い合わせ時に更新し旧許可は元の期限まで維持する

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A132
- verification: unit

DNS情報はアプリからの問い合わせを契機に解決する。有効なキャッシュを使い、期限切れ後の次の問い合わせで取得し直す。問い合わせのない名前を定期的に先読み更新しない。古い応答によるIP許可は元の期限まで維持する。アプリが古いIPだけを保持して名前を再問い合わせしない場合、許可期限後の新規接続は拒否され得る。

## Examples

```gherkin
@id=EX-299 @about=REQ-133 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A132
Scenario: 問い合わせのない名前を先読み更新しない
  Given 一度解決した名前のDNS情報が期限に近づいている
  When その名前についてアプリからの問い合わせがない
  Then 定期的な先読み更新のためには上流へ問い合わせない

@id=EX-300 @about=REQ-133 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A132
Scenario: 新しい応答にない旧IPの許可も元の期限まで残す
  Given 古い応答によるIPの許可に30秒の有効期間が残っている
  When 新しい応答が別のIPだけを返す
  Then 旧IPの許可は元の期限まで残る

```
