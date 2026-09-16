# アドレスの対応範囲

IPv4・IPv6と隔離環境内ループバックの扱いを定義する草案。内部・特殊用途アドレスの分類、ホスト側への接続の具体形は未決。

## 要求

### REQ-030: IPv4とIPv6の対応

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A35
- 検証: unit

初版からIPv4とIPv6の双方を扱い、同じ許可モデルを適用する。両方でIP/CIDR指定とDNS由来の許可を扱う。

### REQ-031: 隔離環境内ループバックの許可

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A36
- 検証: unit

隔離環境内のループバック通信はTCP/UDPの全ポートを既定で許可する。この許可によってホスト側のlocalhostには自動接続させない。

### REQ-032: ホスト側localhostへの明示的な接続許可

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A37
- 検証: unit

初版からホスト側のlocalhostで待ち受けるサービスへの接続を扱う。ホスト側を指す専用の宛先指定とTCP/UDP・ポートの明示で許可する。

## 具体例

```gherkin
@id=EX-051 @about=REQ-030 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A35
Scenario: IPv6の明示許可に一致する通信を通す
  Given IPv6の宛先とTCP・443番への有効な許可がある
  When その宛先のTCP・443番へ新規通信する
  Then 通信を通す

@id=EX-052 @about=REQ-030 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A35
Scenario: IPv6にも指定ポートの制限を適用する
  Given 隔離環境内ループバックではないIPv6の宛先のTCP・443番だけを許可している
  When その宛先のTCP・22番へ新規通信する
  Then 通信を拒否する

@id=EX-053 @about=REQ-031 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A36
Scenario: 隔離環境内の補助サーバーに追加設定なしで接続する
  Given 隔離環境内のループバックで補助サーバーがTCP・3000番を待ち受けている
  And そのポートの明示許可ルールがない
  When 同じ隔離環境内のアプリがそのサーバーへ接続する
  Then 通信を通す

@id=EX-054 @about=REQ-031 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A36
Scenario: 隔離環境のlocalhostをホスト側へ自動転送しない
  Given ホスト側だけでlocalhostのTCP・5432番を待ち受けている
  And ホスト側への接続を明示許可していない
  When 隔離環境内のアプリがlocalhostのTCP・5432番へ接続する
  Then その通信をホスト側サービスへ自動転送しない

@id=EX-055 @about=REQ-032 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A37
Scenario: 明示したホスト側のDBへ接続する
  Given ホスト側のlocalhostのTCP・5432番にDBが待ち受けている
  And ホスト側を指す専用の宛先指定でTCP・5432番を許可した
  When 隔離環境内のアプリがその指定先へ接続する
  Then ホスト側のDBへ接続できる

@id=EX-056 @about=REQ-032 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A37
Scenario: ホスト側の別ポートは許可しない
  Given ホスト側を指す専用の宛先指定でTCP・5432番だけを許可した
  When 隔離環境内のアプリがその指定先のTCP・5433番へ接続する
  Then 通信を拒否する
```
