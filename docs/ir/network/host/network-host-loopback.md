# ホストループバック宛先の設定

ホスト側のlocalhostを明示許可する記法を定義する草案。固定名の提供機構と中継経路は未決。

## 要求

### REQ-089: host-loopbackによる専用宛先の指定

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A94
- 検証: unit

ホストlocalhost専用の宛先は "destination" 内に "host-loopback" を置き、"ipv4" または "ipv6" を指定する。前者はホストの127.0.0.1、後者はホストの::1を指す。両方を許可する場合は2件書く。"dns"・"ip"・"cidr" とは同時指定しない。

### REQ-090: アプリ向けのホスト接続専用名

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A95
- 検証: unit

隔離環境内で "host-v4.kakoi.internal" と "host-v6.kakoi.internal" の固定名を提供し、それぞれホスト127.0.0.1と::1へ指定ポートで中継する。通常の "localhost" は隔離環境内を指すままとする。名前を知っていても、許可したTCP/UDP・ポート以外には接続できない。

### REQ-091: 専用名の通常DNS許可指定の拒否

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A96
- 検証: unit

ホスト接続専用名を通常の "dns" 許可に完全一致で指定したら、"host-loopback" の使用を案内する設定エラーにする。通常DNSのワイルドカードからホストループバックの許可は追加しない。

## 具体例

```gherkin
@id=EX-187 @about=REQ-089 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A94
Scenario: ホストのIPv4ループバックを指定する
  Given network.allowにdestination={host-loopback="ipv4"}とprotocol="tcp"とports=["5432"]を指定している
  When 許可の宛先を解釈する
  Then ホストの127.0.0.1のTCP5432番を許可対象とする

@id=EX-188 @about=REQ-089 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A94
Scenario: ホストのIPv6ループバックを指定する
  Given network.allowにdestination={host-loopback="ipv6"}とprotocol="udp"とports=["9000"]を指定している
  When 許可の宛先を解釈する
  Then ホストの::1のUDP9000番を許可対象とする

@id=EX-189 @about=REQ-089 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A94
Scenario: 専用宛先と通常のIP指定を混ぜない
  Given destinationにhost-loopbackとipを同時に指定している
  When 設定を検査する
  Then 宛先の同時指定を入力エラーにする

@id=EX-190 @about=REQ-090 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A95
Scenario: 固定名からホストのIPv4サービスに接続する
  Given host-loopback="ipv4"でTCP5432番を許可している
  And ホスト127.0.0.1のTCP5432番にサービスがある
  When アプリがhost-v4.kakoi.internalのTCP5432番へ接続する
  Then ホストのそのサービスへ中継する

@id=EX-191 @about=REQ-090 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A95
Scenario: 固定名からホストのIPv6サービスに接続する
  Given host-loopback="ipv6"でUDP9000番を許可している
  And ホスト::1のUDP9000番にサービスがある
  When アプリがhost-v6.kakoi.internalのUDP9000番へ送信する
  Then ホストのそのサービスへ中継する

@id=EX-192 @about=REQ-090 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A95
Scenario: 固定名を知っていても未許可ポートには接続できない
  Given ホストIPv4ループバックの許可はTCP5432番だけである
  When アプリがhost-v4.kakoi.internalのTCP5433番へ接続する
  Then その通信を拒否する

@id=EX-193 @about=REQ-091 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A96
Scenario: 専用名のDNS許可には専用記法を案内する
  Given destination={dns="host-v4.kakoi.internal"}で通信許可を指定している
  When 起動前に設定を検査する
  Then host-loopbackの使用を案内する設定エラーにする

@id=EX-194 @about=REQ-091 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A96
Scenario: ワイルドカードからホストへの許可を増やさない
  Given 通常のDNS許可で*.kakoi.internalのTCP5432番を許可している
  And host-loopbackの許可はない
  When アプリがhost-v4.kakoi.internalのTCP5432番へ接続しようとする
  Then DNSワイルドカードを根拠にそのホストループバック通信を許可しない
```
