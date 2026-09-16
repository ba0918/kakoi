# ホスト公開先のIPv4とIPv6

ホストのループバックで公開するアドレスの選択を定義する草案。設定記法、公開中の片側障害、隔離環境側の転送先選択と中継の実現方式は未決。

後続の動的公開の要求。A158/A159により初版の対象から分離した。本文の初版は動的公開の初回提供を指す。初版の固定公開はnetwork/network-initial-release.mdで定義する。

## 要求

### REQ-072: 公開先のアドレス系統の選択

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A77
- 検証: unit

ホストの公開先は公開ごとにIPv4・IPv6・両方を選択でき、既定はIPv4とする。IPv4は127.0.0.1、IPv6は::1を公開先とする。

### REQ-073: 両方指定時の同番号での公開成立

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A78
- 検証: unit

ホスト公開先にIPv4とIPv6の両方を指定した場合、許可範囲内の同じポート番号を両方で確保してから公開する。共通の空きがなければその公開を失敗として通知・再試行し、アプリと他の公開は継続する。片方だけの公開には自動変更しない。

### REQ-074: 異なるIP系統間の待受公開

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A79
- 検証: unit

待受公開ではホスト側と隔離環境側の異なるIP系統間も中継する。IPv6からIPv4、IPv4からIPv6の両方向をTCP・UDPとも初版の対象とする。

## 具体例

```gherkin
@id=EX-139 @about=REQ-072 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A77
Scenario: 省略時はIPv4だけを公開先にする
  Given 公開先のIPv4とIPv6の選択を省略している
  When ホスト側の公開先を決定する
  Then 127.0.0.1を公開先にする
  And ::1を公開先に追加しない

@id=EX-140 @about=REQ-072 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A77
Scenario: IPv6だけを選べる
  Given 公開先にIPv6を指定している
  When ホスト側の公開先を決定する
  Then ::1を公開先にする
  And 127.0.0.1を公開先に追加しない

@id=EX-141 @about=REQ-072 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A77
Scenario: 両方を選べる
  Given 公開先にIPv4とIPv6の両方を指定している
  When ホスト側の公開先を決定する
  Then 127.0.0.1と::1を公開先にする

@id=EX-142 @about=REQ-073 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A78
Scenario: 両方で確保できる同じ番号を使う
  Given ホスト公開先にIPv4とIPv6の両方を指定している
  And 許可範囲内の45000番を127.0.0.1と::1で確保できる
  When 45000番で公開を開始する
  Then 両方のアドレスで45000番を確保してから公開する

@id=EX-143 @about=REQ-073 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A78
Scenario: 片方にしか空きがなければ部分公開しない
  Given ホスト公開先にIPv4とIPv6の両方を指定している
  And 許可範囲内にIPv4とIPv6で共通に使える空き番号がない
  When 待受を公開しようとする
  Then 片方だけでは公開せずその公開の失敗を通知して再試行する
  And アプリと他の公開を継続する

@id=EX-144 @about=REQ-074 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A79
Scenario: ホストのIPv6からIPv4のみのTCP待受に届く
  Given 隔離環境内のIPv4のみのTCP待受をホストのIPv6で公開している
  When ホストのIPv6公開先にTCP接続する
  Then 指定された隔離環境内のIPv4待受へ中継する

@id=EX-145 @about=REQ-074 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A79
Scenario: ホストのIPv4からIPv6のみのUDP待受に届く
  Given 隔離環境内のIPv6のみのUDP待受をホストのIPv4で公開している
  When ホストのIPv4公開先へUDPデータグラムを送る
  Then 指定された隔離環境内のIPv6待受へ中継する
```
