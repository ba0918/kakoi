# 対話中の割り込み

filteredでのCtrl+Cの意味を定義する草案。シグナル配送の実現方式は未決。外部からの終了要求はprocess-termination.mdで定義する。

## Requirements

### REQ-098: 対話中のCtrl+Cをアプリ側へ届ける

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A103
- verification: unit

"filtered" の対話中のCtrl+Cはアプリ側へ割り込みを届ける。アプリが操作をキャンセルして続行するなら隔離環境も継続し、主コマンドが終了したら決定済みの片付けを行う。

### REQ-099: 終了猶予中のCtrl+Cで待機を打ち切る

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A104
- verification: unit

"filtered" で主コマンド終了後の終了猶予中にCtrl+Cが来たら、残りの猶予を打ち切り、残った子プロセスを強制終了して片付けを進める。

### REQ-100: 猶予打切りで主コマンドの結果を変えない

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A105
- verification: unit

主コマンド終了後の猶予をCtrl+Cで打ち切っても、確定済みの主コマンドの終了結果を維持する。猶予を打ち切ったことは通知する。

## Examples

```gherkin
@id=EX-214 @about=REQ-098 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A103
Scenario: 操作だけをキャンセルするアプリを終了させない
  Given filteredで主コマンドが対話中である
  And アプリはCtrl+Cを操作のキャンセルとして扱い実行を継続する
  When 利用者がCtrl+Cを押す
  Then アプリ側へ割り込みを届ける
  And 主コマンドの続行に合わせて隔離環境も継続する

@id=EX-215 @about=REQ-098 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A103
Scenario: 割り込みで主コマンドが終了したら片付ける
  Given filteredで主コマンドが対話中である
  When Ctrl+Cによる割り込みを受けた主コマンドが終了する
  Then 決定済みの隔離環境の終了処理を行う

@id=EX-216 @about=REQ-099 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A104
Scenario: 長い終了猶予を利用者が打ち切る
  Given filteredで主コマンドは終了している
  And 残った子プロセスの終了猶予があと200秒ある
  When 利用者がCtrl+Cを押す
  Then 残り200秒を待たずに残った子プロセスを強制終了する
  And 片付けを進める

@id=EX-217 @about=REQ-100 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A105
Scenario: 成功後の片付けを打ち切っても成功結果を返す
  Given 主コマンドは終了コード0で終了し子プロセスの終了猶予中である
  When 利用者がCtrl+Cで猶予を打ち切る
  Then 猶予打切りを通知する
  And 主コマンドの終了コード0を返す

@id=EX-218 @about=REQ-100 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A105
Scenario: 失敗後の片付けを打ち切っても元の失敗結果を返す
  Given 主コマンドは終了コード7で終了し子プロセスの終了猶予中である
  When 利用者がCtrl+Cで猶予を打ち切る
  Then 猶予打切りを通知する
  And 主コマンドの終了コード7を返す
```
