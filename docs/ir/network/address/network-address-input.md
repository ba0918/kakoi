# IPアドレスとCIDRの入力解釈

IPv4-mapped IPv6の判定とCIDRのホスト部を定義する草案。その他のIPv4埋め込み形式は未決。IPv4の入力書式とプレフィックス長の範囲は別のIRで定義する。

## 要求

### REQ-044: IPv4-mapped IPv6の判定統一

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A49
- 検証: unit

IPv4-mapped IPv6（::ffff:0:0/96）のアドレスは対応するIPv4にそろえ、許可照合と内部IP分類を行う。表記を変えても許可範囲と内部IPの制限を変えない。

### REQ-045: CIDRの非ゼロのホスト部の拒否

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A50
- 検証: unit

IPv4・IPv6のCIDR指定でホスト部が非ゼロの場合は入力エラーとし、ホスト部をゼロにした修正候補を示す。自動修正して受け付けない。

### REQ-046: 全アドレスを表すCIDR指定

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A51,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A139
- 検証: unit

"0.0.0.0/0" と "::/0" を明示的なCIDR指定として認める。それぞれIPv4全体・IPv6全体を表し、内部IPも指定TCP/UDP・ポートの明示許可に含む。この指定によって隔離や待受公開の設定、他の通信方式の制限を解除しない。IPv6リンクローカルはnetwork/link-local/network-link-local-wide-cidr.mdの接続口指定条件にも従う。

### REQ-047: IPv4-mapped形式のCIDRの変換

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A52
- 検証: unit

範囲全体がIPv4-mapped領域（::ffff:0:0/96）内に収まるCIDRは、対応するIPv4 CIDRに変換して受け付ける。IPv6プレフィックス長96〜128をIPv4の0〜32へ対応させる。非ゼロのホスト部を持つ入力は、変換して受け付けず入力エラーにする。

## 具体例

```gherkin
@id=EX-080 @about=REQ-044 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A49
Scenario: IPv4の許可を対応するIPv4-mapped表記にも適用する
  Given 192.168.1.10のTCP・443番を明示許可している
  When ::ffff:192.168.1.10のTCP・443番への通信許可を判定する
  Then 同じIPv4宛先への許可に一致すると判定する

@id=EX-081 @about=REQ-044 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A49
Scenario: IPv4-mapped表記でも内部IPの検査を適用する
  Given DNS名の許可だけがあり192.168.1.10へのIPまたはCIDRの明示許可がない
  When ::ffff:192.168.1.10への新規通信許可を判定する
  Then 内部IPへの明示許可がないため拒否する

@id=EX-082 @about=REQ-045 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A50
Scenario: IPv4の非ゼロのホスト部を拒否する
  Given CIDR指定が "192.168.1.23/24" である
  When 入力を検査する
  Then 入力エラーとし "192.168.1.0/24" への修正を案内する
  And 自動修正して受け付けない

@id=EX-083 @about=REQ-045 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A50
Scenario: IPv6の非ゼロのホスト部も拒否する
  Given CIDR指定が "fd00::1/64" である
  When 入力を検査する
  Then 入力エラーとし "fd00::/64" への修正を案内する
  And 自動修正して受け付けない

@id=EX-084 @about=REQ-046 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A51
Scenario: 全IPv4の指定は内部IPの明示許可にもなる
  Given "0.0.0.0/0" のTCP・443番を明示許可している
  When 192.168.1.10のTCP・443番への新規通信許可を判定する
  Then その明示許可に一致すると判定する

@id=EX-085 @about=REQ-046 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A51
Scenario: 全アドレス指定でもポート制限を維持する
  Given "::/0" のTCP・443番だけを明示許可している
  When 隔離環境内ループバックではないIPv6宛先のTCP・22番へ新規通信する
  Then 通信を拒否する

@id=EX-086 @about=REQ-047 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A52
Scenario: IPv4-mappedの範囲をIPv4の範囲へ変換する
  Given CIDR指定が "::ffff:192.168.1.0/120" である
  When 許可ルールを読み込む
  Then "192.168.1.0/24" として受け付ける

@id=EX-087 @about=REQ-047 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A52
Scenario: IPv4-mappedのCIDRでも非ゼロのホスト部を拒否する
  Given CIDR指定が "::ffff:192.168.1.23/120" である
  When 許可ルールを読み込む
  Then ホスト部が非ゼロのため入力エラーにする
```
