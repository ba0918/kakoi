# 通信許可の設定形式

一般宛先の許可の記法を定義する草案。ホストlocalhost専用の宛先はnetwork/host/network-host-loopback.mdを参照する。

## Requirements

### REQ-088: 通信許可の項目と宛先の記法

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A93
- verification: unit

通信許可はTOMLの "[[network.allow]]" に "destination"・"protocol"・"ports" を記述し、3項目とも必須とする。一般宛先は "destination" 内の "dns"・"ip"・"cidr" のいずれか1つで指定する。"protocol" は "tcp" または "udp"、"ports" はnetwork-ports.mdで定める文字列配列を使う。IPv6リンクローカルの "host-interface" は "destination" 内に置く。

### REQ-092: 通信許可リストを段の間で連結

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A97
- verification: unit

"network.allow" は段の間で追加して合成し、各許可のいずれかに一致する通信を許可する。重複はまとめ、空配列では元の許可を消さない。許可を減らす場合は元の設定を編集するか別プロファイルを使う。

## Examples

```gherkin
@id=EX-183 @about=REQ-088 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A93
Scenario: DNS名とTCPポートを指定する
  Given network.allowの項目にdestination={dns="api.example.com"}とprotocol="tcp"とports=["443"]を記述している
  When 通信許可を解釈する
  Then api.example.comのTCP443番の許可として扱う

@id=EX-184 @about=REQ-088 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A93
Scenario: 宛先種別の同時指定を拒否する
  Given network.allowのdestinationにdnsとipを両方指定している
  When 通信許可を検査する
  Then 宛先種別の同時指定を入力エラーにする

@id=EX-185 @about=REQ-088 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A93
Scenario: リンクローカルのインターフェースを宛先内に指定する
  Given destination={ip="fe80::1", host-interface="eth0"}とprotocol="tcp"とports=["443"]を記述している
  When 通信許可の入力形式を検査する
  Then eth0を指定したリンクローカル宛先の形式として受け付ける

@id=EX-186 @about=REQ-088 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A93
Scenario: protocolの省略を拒否する
  Given network.allowの項目にdestinationとportsがありprotocolがない
  When 通信許可を検査する
  Then 必須項目の欠落として入力エラーにする

@id=EX-195 @about=REQ-092 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A97
Scenario: 別の段の通信許可を追加する
  Given filteredで下の段にAPIへのTCP443番の許可がある
  And 上の段にホストIPv4ループバックのTCP5432番の許可がある
  When 許可設定を合成する
  Then 両方の許可を残す

@id=EX-196 @about=REQ-092 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A97
Scenario: 空配列を許可の削除と解釈しない
  Given 下の段に通信許可がある
  And 上の段のnetwork.allowは空配列である
  When 許可設定を合成する
  Then 下の段の許可を維持する

@id=EX-197 @about=REQ-092 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A97
Scenario: 同じ許可の重複をまとめる
  Given 2つの段に同じ宛先とprotocolとportsの許可がある
  When 許可設定を合成する
  Then その重複を1つの許可にまとめる
```
