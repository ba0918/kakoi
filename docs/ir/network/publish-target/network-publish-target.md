# 公開対象の区別

同じ番号の別々の待受を公開する場合の草案。待受の識別方式、共有先の構成変化時の寿命、ポート割当順序は未決。

後続の動的公開の要求。A158/A159により初版の対象から分離した。本文の初版は動的公開の初回提供を指す。初版の固定公開はnetwork/network-initial-release.mdで定義する。

## 要求

### REQ-075: 別々のIPv4とIPv6の待受の公開を分ける

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A80
- 検証: unit

隔離環境内の同じTCP/UDP・ポート番号でIPv4とIPv6に別々の待受があり両方が公開対象の場合、別々のホスト公開ポートを許可範囲内で割り当て、各転送先との対応を知らせる。

### REQ-076: 待受ソケット単位の公開

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A81, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A82
- 検証: unit

公開対象は待受ソケット単位で数える。単一ソケットがIPv4とIPv6の両方を受けるなら1件、同じプロセスでも別々のソケットなら別件として公開する。ただし、同じIP・TCP/UDP・ポートを共有する待受ソケット群は例外として1件にまとめる。

### REQ-077: 共有する受付先の公開と配送

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A82
- 検証: unit

同じIP・TCP/UDP・ポートを複数の待受ソケットで共有する場合は、共有先を1件の公開にまとめ、各ソケットへの配送はOSに任せる。

## 具体例

```gherkin
@id=EX-146 @about=REQ-075 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A80
Scenario: 同じ番号でも別のサービスには別の公開番号を使う
  Given 隔離環境の127.0.0.1:8000と[::1]:8000に別々のTCP待受がある
  And 両方が公開対象でホストの許可範囲内に十分な空きがある
  When 両方の待受を公開する
  Then それぞれに異なるホスト公開ポートを割り当てる
  And 各公開先と隔離環境内の転送先の対応を通知する

@id=EX-147 @about=REQ-075 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A80
Scenario: 公開先のIP系統によって別サービスへ振り分けない
  Given 同じTCPポート番号に別々のIPv4とIPv6の待受があり両方が公開対象である
  And 各公開先にはホストのIPv4とIPv6の両方を指定している
  And ホストの許可範囲内に十分な空きがある
  When 両方の待受を公開する
  Then 各公開番号のIPv4とIPv6はその番号に対応する同じ待受へ中継する
  And 2つの待受を1つの公開番号のIPv4側とIPv6側へまとめない

@id=EX-148 @about=REQ-076 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A81
Scenario: 単一の待受をIP系統ごとに二重公開しない
  Given 単一の公開対象ソケットがIPv4とIPv6の両方を受け付ける
  When 公開対象を数える
  Then 1件の公開として扱う

@id=EX-149 @about=REQ-076 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A81
Scenario: 同じプロセスでも別のソケットはまとめない
  Given 同じプロセスが別々の公開対象ソケットで127.0.0.1:8000と[::1]:8000を待ち受けている
  When 公開対象を数える
  Then 2件の公開として扱う

@id=EX-150 @about=REQ-077 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A82
Scenario: 同じ受付先を共有するソケット群をまとめる
  Given 公開対象の複数のTCP待受ソケットが同じIPとポートを共有している
  When その共有先を公開する
  Then ソケットごとに公開番号を増やさず1件として公開する
  And 各ソケットへの配送はOSに任せる

@id=EX-151 @about=REQ-077 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A82
Scenario: UDPの共有先もソケットごとに分けない
  Given 公開対象の複数のUDPソケットが同じIPとポートを共有している
  When その共有先を公開する
  Then 1件として公開する
  And データグラムの各ソケットへの配送はOSに任せる
```
