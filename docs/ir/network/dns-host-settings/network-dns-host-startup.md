# 必要なホストDNSを引き継げない場合の起動拒否

起動時の扱いと、ホストの名前解決設定から上流を選ぶ規則。

## Requirements

### REQ-145: 必要なホストDNSを引き継げない場合の起動拒否

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A143, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A168
- verification: unit

DNS名の許可を使う "filtered" でホストDNSを選び、起動時にホストの名前解決設定を読めない、または "nameserver" が一つも無い場合は起動エラーとし、上流DNSの明示指定を案内する。勝手に公開DNSへ切り替えない。DNS名の許可を使わない構成までホストDNSを必須にしない。起動後の設定取得失敗はnetwork-dns-settings-failure.mdに従う。

### REQ-396: ホストの名前解決設定から上流を選ぶ

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A168
- verification: unit

ホストDNSは、ホストの名前解決設定の "nameserver" を記載順に通常DNSの上流として使う。記載がsystemd-resolvedのスタブ127.0.0.53だけの場合は、中継窓口127.0.0.54を使う。

### REQ-417: ホストのループバック上のDNSを上流に使う

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A14
- verification: unit

ホストDNSの上流への問い合わせは、隔離環境の中ではなくkakoi自身のネットワークから送る。このため、ホストの名前解決設定の "nameserver" がループバックのアドレスであれば、ホストのループバック上のDNSを上流に使う。

## Examples

```gherkin
@id=EX-322 @about=REQ-145 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A143
Scenario: 必要なホストDNSを引き継げなければアプリを起動しない
  Given filteredでDNS名の許可を使いホストDNSを選んでいる
  When 起動時にホストの名前解決設定にnameserverが一つも無い
  Then 起動エラーにして上流DNSの明示指定を案内する
  And 勝手に公開DNSへ切り替えない

@id=EX-323 @about=REQ-145 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A143
Scenario: DNS名の許可がない空のfilteredにまでDNS対応を要求しない
  Given filteredの許可と公開設定は空でDNS名の許可はない
  When ホストDNS構成が非対応である
  Then ホストDNSが非対応であることだけを理由に起動を拒否しない


@id=EX-731 @about=REQ-396 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A168
Scenario: ホストの記載どおりの問い合わせ先を使う
  Given ホストの名前解決設定にnameserver 10.255.255.254とnameserver 192.0.2.53がこの順で書かれている
  When アプリの名前解決のために上流へ問い合わせる
  Then 10.255.255.254と192.0.2.53を記載順に通常DNSで使う

@id=EX-732 @about=REQ-396 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A168
Scenario: systemd-resolvedのスタブだけなら中継窓口を使う
  Given ホストの名前解決設定のnameserverは127.0.0.53だけである
  When アプリの名前解決のために上流へ問い合わせる
  Then 127.0.0.54へ問い合わせる

@id=EX-796 @about=REQ-417 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A14
Scenario: ホストのループバック上のDNSへ問い合わせる
  Given ホストの名前解決設定の "nameserver" は 127.0.0.1 だけで、ホストの 127.0.0.1 の53番でDNSが待ち受けている
  When アプリの名前解決のために上流へ問い合わせる
  Then ホストの 127.0.0.1 のDNSへ問い合わせて応答を得る

@id=EX-797 @about=REQ-417 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A14
Scenario: ループバックの上流を隔離環境の中のアドレスとして扱わない
  Given ホストの名前解決設定の "nameserver" は 127.0.0.1 だけである
  When アプリの名前解決のために上流へ問い合わせる
  Then 隔離環境の中の 127.0.0.1 へは問い合わせを送らない
```
