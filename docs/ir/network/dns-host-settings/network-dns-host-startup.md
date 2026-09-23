# 必要なホストDNSを引き継げない場合の起動拒否

起動時の扱いと初期対応の方針。具体的な対応版と判定方法は実証待ち。

## Requirements

### REQ-145: 必要なホストDNSを引き継げない場合の起動拒否

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A143, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A156
- verification: unit

DNS名の許可を使う "filtered" でホストDNSを選び、起動時にその構成を安全に引き継げないと判明した場合は起動エラーとし、上流DNSの明示指定を案内する。勝手に公開DNSへ切り替えない。DNS名の許可を使わない構成までホストDNS対応を必須にしない。起動後の設定取得失敗はnetwork-dns-settings-failure.mdに従う。

初期対応は、systemd-resolvedの中継窓口127.0.0.54を使う実証済みの構成から始める。その他の構成は上流DNSの明示指定を案内する。全systemd-resolved構成を対応済みとはせず、対応版・実際のホスト設定との一致・設定追従の判定方法は実証で確定する。

## Examples

```gherkin
@id=EX-322 @about=REQ-145 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A143
Scenario: 必要なホストDNSを引き継げなければアプリを起動しない
  Given filteredでDNS名の許可を使いホストDNSを選んでいる
  When 起動時にその構成を安全に引き継げないと判明する
  Then 起動エラーにして上流DNSの明示指定を案内する
  And 勝手に公開DNSへ切り替えない

@id=EX-323 @about=REQ-145 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A143
Scenario: DNS名の許可がない空のfilteredにまでDNS対応を要求しない
  Given filteredの許可と公開設定は空でDNS名の許可はない
  When ホストDNS構成が非対応である
  Then ホストDNSが非対応であることだけを理由に起動を拒否しない

```
