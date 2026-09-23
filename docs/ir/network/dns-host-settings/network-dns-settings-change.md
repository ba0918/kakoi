# 実行中のホストDNS設定変更

ホストのDNS設定への追従を定義する草案。変更検知の機構と遅延は未決。変更後の設定取得失敗はnetwork-dns-settings-failure.mdで定義する。

## Requirements

### REQ-107: ホストDNSの変更後に開始する問い合わせへ反映する

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A112
- verification: unit

ホストのDNS設定を使う場合、実行中も設定変更に追従し、変更を検知した後に開始する上流への問い合わせには新しい設定を使う。ポリシーで明示指定した上流はホストのDNS設定変更への追従の対象外とする。

### REQ-108: DNS設定変更時は古いキャッシュを破棄し既存許可を維持する

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A113
- verification: unit

ホストDNS設定変更を検知したら、kakoi-netが保存した古いDNS応答を破棄し、次の名前解決は新しい設定で行う。すでに与えたIP許可は元の期限まで維持し、確立済みTCPと継続中UDPは既存の継続規則で扱う。アプリ自身が保存した解決結果まで消す保証はしない。

### REQ-109: 旧設定の未完了問い合わせを新設定でやり直す

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A114
- verification: unit

ホストDNS設定変更の検知後に届く旧設定の問い合わせ応答は採用せず、新設定で問い合わせ直す。旧応答をアプリへ返さず、キャッシュやIP許可の追加・更新にも使わない。問い合わせ直す場合も元の解決処理の時間・作業量上限を引き継ぎ、設定変更が続いても無制限には延長しない。

## Examples

```gherkin
@id=EX-232 @about=REQ-107 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A112
Scenario: 実行中に変更された問い合わせ先を使う
  Given ホストのDNS設定を使っている
  And ホストの名前別の問い合わせ先の変更を検知した
  When 変更の対象となる許可名について新たに上流問い合わせを開始する
  Then 新しいホスト設定の問い合わせ先を使う
  And kakoiの再起動を必要としない

@id=EX-233 @about=REQ-107 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A112
Scenario: 明示指定した上流はホストの変更に追従しない
  Given ポリシーで上流DNSを明示指定している
  When ホストのDNS設定が変更される
  Then ポリシーで指定した上流DNSを維持する

@id=EX-234 @about=REQ-108 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A113
Scenario: 期限内のキャッシュでも設定変更後の解決には使わない
  Given kakoi-netが古いホストDNS設定で得た応答を期限内のキャッシュとして保持している
  When ホストDNS設定の変更を検知した後にその名前を解決する
  Then 古いキャッシュを使わず新しい設定で名前解決する

@id=EX-235 @about=REQ-108 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A113
Scenario: キャッシュを破棄しても既存のIP許可は元の期限まで残る
  Given 古いDNS応答から得た検査済みIPの許可期限まで30秒残っている
  When ホストDNS設定の変更を検知して古いキャッシュを破棄する
  Then そのIP許可は元の期限まで残り30秒のまま維持する

@id=EX-236 @about=REQ-108 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A113
Scenario: 設定変更だけでは継続中の通信を切らない
  Given DNS由来の許可でTCP接続が確立しUDP通信も継続している
  When ホストDNS設定の変更を検知する
  Then 確立済みTCPは既存の継続規則で維持する
  And 継続中UDPは既存の無通信期限の規則で維持する

@id=EX-237 @about=REQ-109 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A114
Scenario: 切替後に届いた旧設定の応答を採用しない
  Given 旧設定で開始したDNS問い合わせが未完了である
  When ホストDNS設定変更の検知後に旧設定の応答が届く
  Then その応答をアプリへ返さない
  And キャッシュやIP許可の追加と更新に使わない
  And 新設定で問い合わせ直す

@id=EX-238 @about=REQ-109 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A114
Scenario: 問い合わせ直しても解決処理の上限を数え直さない
  Given DNS解決処理が時間と作業量の上限の一部を消費している
  When 設定変更により新設定で問い合わせ直す
  Then 元の処理の期限と残り作業量を引き継ぐ
  And 再び設定が変わっても上限をリセットしない
```
