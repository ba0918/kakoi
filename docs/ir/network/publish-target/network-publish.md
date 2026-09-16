# 隔離環境内サービスの公開

ホストのブラウザー等から隔離環境内サービスへ接続するための公開を定義する草案。待受検出と公開の同期、設定形式、公開ポート確保失敗時の扱い、公開URL表示、公開の寿命は未決。

後続の動的公開の要求。A158/A159により初版の対象から分離した。本文の初版は動的公開の初回提供を指す。初版の固定公開はnetwork/network-initial-release.mdで定義する。

## 要求

### REQ-033: ホストのlocalhostへの待受公開

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A38
- 検証: unit

初版から隔離環境内サービスの待受ポートを公開できるようにし、公開先はホストのlocalhostに限定する。隔離環境内で起動したkemiのレビュー画面をホストのブラウザーから開く用途を満たす。LAN等への公開は初版に含めない。

### REQ-034: 事前許可範囲内での動的ポート選択

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A39
- 検証: unit

公開ポートは事前に許可した範囲内で空きポートを動的に選べるようにする。アプリの申告による事前定義外への拡張は認めない。

### REQ-035: 許可範囲内の待受の自動公開

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A40
- 検証: unit

事前許可した範囲内でサービスの待受を検出したら、ホストのlocalhostへ自動公開する。アプリ固有の公開登録を必要としない。同じ許可範囲内で待ち受ける別のサービスも公開対象とする。

### REQ-036: 公開ポートの衝突回避

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A41, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A39, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A75
- 検証: unit

障害復帰時に同じ待受が継続し以前の公開番号が使用可能な場合は、その番号を優先する。それ以外はホスト側の事前許可範囲内で、隔離環境側と同じポート番号を優先する。同じ番号が使用中なら範囲内で別の空きポートを選ぶ。同じ番号が範囲外の場合も、範囲内から選ぶ。実際の公開先をkakoiが知らせる。

## 具体例

```gherkin
@id=EX-057 @about=REQ-033 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A38
Scenario: ホストから隔離環境内のレビュー画面を開く
  Given 隔離環境内のkemiの待受ポートを明示的な許可に従って公開した
  When ホストのブラウザーで公開先のlocalhostのURLを開く
  Then レビュー画面を閲覧できる

@id=EX-058 @about=REQ-033 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A38
Scenario: 公開をLAN向けの待受にしない
  Given 隔離環境内サービスの待受公開を設定した
  When ホスト側の公開先を作る
  Then 公開先をホストのlocalhostに限定する
  And LAN向けのアドレスでは公開しない

@id=EX-059 @about=REQ-034 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A39
Scenario: 事前許可範囲内で空きポートを選ぶ
  Given 公開ポートの範囲が事前に許可されており範囲内に空きがある
  When 公開ポートを動的に選ぶ
  Then 事前許可範囲内の空きポートを選ぶ

@id=EX-060 @about=REQ-034 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A39
Scenario: アプリの申告で公開範囲を広げない
  Given 公開ポートの範囲が事前に確定している
  When アプリがその範囲外のポートを公開するよう申告する
  Then その申告によって公開範囲を拡張しない

@id=EX-061 @about=REQ-035 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A40
Scenario: 許可範囲内の待受を登録操作なしで公開する
  Given 公開するポート範囲が事前に許可されている
  And ホスト側の公開ポートを確保できる
  When その範囲内でサービスの待受を検出する
  Then アプリ固有の公開登録なしでホストのlocalhostへ公開する

@id=EX-062 @about=REQ-035 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A40
Scenario: 同じ許可範囲ならkemi以外も公開対象になる
  Given 公開するポート範囲が事前に許可されている
  And ホスト側の公開ポートを確保できる
  When その範囲内でkemi以外のサービスの待受を検出する
  Then そのサービスも自動公開する

@id=EX-063 @about=REQ-035 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A40
Scenario: 許可範囲外の待受は自動公開しない
  Given 公開するポート範囲が事前に確定している
  When その範囲外でサービスの待受を検出する
  Then その待受を自動公開しない

@id=EX-064 @about=REQ-036 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A41
Scenario: 同じ番号が使われていれば許可範囲内の別ポートを使う
  Given 隔離環境側の待受ポートと同じ番号がホスト側で使用中である
  And ホスト側の事前許可範囲内に別の空きポートがある
  When 待受を公開する
  Then 許可範囲内の別の空きポートで公開する
  And 実際の公開先を通知する

@id=EX-065 @about=REQ-036 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A41
Scenario: 同じ番号が許可範囲内で空いていれば優先する
  Given 隔離環境側の待受ポートと同じ番号がホスト側の許可範囲内で空いている
  When 初めて待受を公開する
  Then 同じ番号で公開する
```
