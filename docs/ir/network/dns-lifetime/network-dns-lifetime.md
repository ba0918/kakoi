# DNS由来の許可の寿命

DNS応答のTTLとIP許可の寿命、TCP・UDPの継続中の通信の扱いを定義する草案。UDP設定はnetwork-udp-settings.md、更新の契機はnetwork-dns-refresh.mdで定義する。TTLゼロの具体値は合意済みで、機構の実証は別途必要。

## Requirements

### REQ-014: DNS応答の期限に連動する許可

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A15, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A157
- verification: unit

DNS応答から得たIPを検査してからTTLの期間だけ許可する。TTLが0の場合はREQ-389の有限猶予を適用する。新しい応答を得たら再検査して許可を更新する。更新できず期限が切れたIPへの新規通信は、他の有効な許可がなければ拒否する。利用者が解決先IPを追いかけて書き換える操作を必要としない。

### REQ-015: 確立済みTCP接続の維持

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A16
- verification: unit

DNS由来のIP許可が期限切れになっても、すでに確立しているTCP接続は維持する。接続が続く間は古いIPと通信でき、TTLだけを理由に通信を中断しない。

### REQ-016: 継続中のUDP通信の維持

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A17
- verification: unit

DNS由来のIP許可が期限切れになっても、送信元・宛先のIPとポートが同じ継続中のUDP通信は、無通信期限まで維持する。通信が続く間は古いIPと通信できる。IPまたはポートが変わる通信は、新規通信として許可を再判定する。

### REQ-017: UDP無通信期限の変更

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A18, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A20
- verification: unit

UDPの無通信期限は最後に許可した送受信から既定120秒とし、ポリシーファイルの "network" の "udp-idle-timeout-seconds" で変更できる。指定は正の整数秒とし、0と無制限の指定は認めない。省略時は120秒。そのポリシーのUDP許可すべてに共通で適用する。期限後の通信は新規通信として有効な許可を再判定する。

### REQ-018: UDPの同じ組の再利用

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A19
- verification: unit

無通信期限内に同じ送受信IP・ポートを別の処理が再利用しても、同じ継続中のUDP通信として扱う。

### REQ-022: CNAME経由の許可期限

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A27, docs/decision/brainstorm/2026-09-15-kakoi-net.md#D8
- verification: unit

CNAME経由の新規通信許可の期限は、参照経路のCNAMEと最終IP情報のうち最も早い失効時点までとする。TTLゼロの情報にはREQ-389の猶予を適用し、正のTTLの期限は延ばさない。

### REQ-389: TTLゼロの有限猶予

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A21, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A157, docs/decision/brainstorm/2026-09-15-kakoi-net.md#D8
- verification: unit

TTLが0のDNS応答で検査を通ったIPへの新規通信には、応答の受信から既定1000ミリ秒の猶予を設ける。"network.dns-zero-ttl-grace-milliseconds" は100〜10000の整数ミリ秒で指定し、上位の明示値を優先、省略時は下位の値を引き継ぐ。検査・登録の遅れで期限を数え直さず、期限切れの候補を後から有効にしない。許可の更新は新たに検査を通った応答だけで行い、通常の通信では延長しない。TTLゼロの応答を後の問い合わせに再利用しない。猶予中に始まったTCP・UDPには既存の継続規則を適用する。CNAME経路ではTTLゼロの情報にのみ猶予を適用し、各情報の失効時点の最小値を使う。正のTTLの失効を猶予で延ばさない。

### REQ-397: 許可は期限より後に切れない

- kind: invariant
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A170, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A165
- verification: unit

DNS由来の許可は、応答の受信とTTLから決まる期限より後にカーネルで失効させない。登録の遅れに備えて見込んだ時間の分だけ期限より早く失効することは認める。

### REQ-398: アプリへ返す応答のTTLを最短に揃える

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A171
- verification: unit

アドレスの問い合わせへの応答をアプリへ返すとき、回答部の各レコードのTTLを、その応答の回答部で最も短いTTLに揃える。メッセージ署名付きの応答は元のTTLのまま返す。

## Examples

```gherkin
@id=EX-023 @about=REQ-014 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A15
Scenario: 有効な許可がある新規通信を通す
  Given DNS由来の許可が期限内で検査済みである
  When その許可に一致する新規通信を行う
  Then 通信を通す

@id=EX-024 @about=REQ-014 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A15
Scenario: 更新できなかった宛先への新規通信を拒否する
  Given DNS由来の許可が更新できず期限切れになり他の有効な許可もない
  When その宛先へ新規通信を行う
  Then 通信を拒否する

@id=EX-025 @about=REQ-015 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A16
Scenario: TTLだけでは確立済みTCP接続を止めない
  Given DNS由来の許可でTCP接続が確立している
  When その許可が期限切れになる
  Then そのTCP接続の通信を維持する

@id=EX-026 @about=REQ-016 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A17
Scenario: TTLだけでは継続中のUDP通信を止めない
  Given DNS由来の許可でUDP通信が継続中で無通信期限に達していない
  When その許可が期限切れになる
  Then 同じ送受信IP・ポートのUDP通信を維持する

@id=EX-027 @about=REQ-016 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A17,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A15
Scenario: 継続中でも別の組には許可を引き継がない
  Given UDP通信が継続中で宛先のDNS由来の許可は期限切れになり他の有効な許可もない
  When 送信元ポートを変えて通信する
  Then 新規通信として判定し拒否する

@id=EX-028 @about=REQ-017 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A18,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A15
Scenario: 無通信期限後は許可を再判定する
  Given UDP通信の最後に許可した送受信から無通信期限が経過し有効な許可もない
  When 同じ送受信IP・ポートで通信する
  Then 新規通信として判定し拒否する

@id=EX-029 @about=REQ-018 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A19
Scenario: 期限内の同じ組の再利用を継続として扱う
  Given UDP通信の無通信期限に達していない
  When 別の処理が同じ送受信IP・ポートを再利用する
  Then 同じ継続中の通信として扱う

@id=EX-036 @about=REQ-022 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A27
Scenario: 別名参照が先に失効する場合の許可期限
  Given 残り60秒のCNAMEと残り300秒の最終IP情報を同時に得た
  When その参照経路から新規通信の許可を作る
  Then 許可期限を60秒後とする

@id=EX-037 @about=REQ-022 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A27
Scenario: 最終IP情報が先に失効する場合の許可期限
  Given 残り300秒のCNAMEと残り60秒の最終IP情報を同時に得た
  When その参照経路から新規通信の許可を作る
  Then 許可期限を60秒後とする

@id=EX-718 @about=REQ-389 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A157,docs/decision/brainstorm/2026-09-15-kakoi-net.md#D8,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A15
Scenario: 受信時点を猶予の起点にする
  Given 既定設定でTTLゼロの応答を受信した
  When 受信から300ミリ秒後に検査が完了する
  Then 許可期限は受信から1000ミリ秒後であり検査完了からではない

@id=EX-719 @about=REQ-389 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A157,docs/decision/brainstorm/2026-09-15-kakoi-net.md#D8,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A15
Scenario: 通常の通信で新規許可を延長しない
  Given TTLゼロの応答による猶予が切れ他の有効な許可はない
  When 既存の通信が続く間に別の新規通信を開始する
  Then 新規通信を拒否し既存の通信には継続規則を適用する

@id=EX-720 @about=REQ-389 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A157,docs/decision/brainstorm/2026-09-15-kakoi-net.md#D8,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A15
Scenario: 猶予の範囲外の設定を拒否する
  Given TTLゼロ猶予を99ミリ秒または10001ミリ秒に指定した
  When 設定を検査する
  Then 範囲外として拒否する

@id=EX-721 @about=REQ-389 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A157,docs/decision/brainstorm/2026-09-15-kakoi-net.md#D8,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A15
Scenario: 期限後の登録完了では許可を復活させない
  Given 既定設定で受信したTTLゼロ応答の検査を通った
  When 受信から1000ミリ秒より後に許可登録が完了する
  Then その候補を新規通信の有効な許可にしない

@id=EX-733 @about=REQ-397 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A170
Scenario: 登録が遅れても許可を期限より後まで残さない
  Given カーネルへの許可の登録が見込んだ時間を超えて遅れる
  When 見込む時間を延ばして許可を登録し直す
  Then カーネルの許可は応答の受信とTTLから決まる期限までに失効する

@id=EX-734 @about=REQ-398 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A171
Scenario: TTLの違うアドレスを最短のTTLで返す
  Given 上流の応答が許可された二つのIPをTTL30秒と300秒で返す
  When その応答をアプリへ返す
  Then アプリが受け取る二つのIPのTTLはどちらも30秒以下である

@id=EX-735 @about=REQ-398 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A171
Scenario: 別名の短いTTLに最終IPのTTLを揃える
  Given 別名のTTLが10秒で最終IPのTTLが300秒である
  When その応答をアプリへ返す
  Then アプリが受け取る最終IPのTTLは10秒以下である

```
