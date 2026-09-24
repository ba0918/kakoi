# ネットワーク状態の所有と終了

隔離環境ごとの所有を定義する草案。異常終了の検知・回収方式は未決。終了猶予の設定の配置と期限の起点はnetwork/process/process-shutdown-config.mdとnetwork/process/process-shutdown-deadline.mdで定義する。

終了猶予はfilteredだけに適用する。host・noneの終了方式は既存どおりとし、適用条件はnetwork/process/process-shutdown-config.mdに従う。

## Requirements

### REQ-060: 隔離環境ごとの独立管理と回収

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A65
- verification: unit

ネットワーク状態と補助処理は隔離環境ごとに独立して管理する。環境終了時に、その環境のIP許可・公開ポート・補助プロセスを片付ける。他の稼働中の隔離環境の通信や公開には影響させない。

### REQ-061: 主コマンド終了時の環境終了

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A66
- verification: unit

主コマンドが終了したら隔離環境も終了する。残った子プロセスも終了し、公開ポートを片付ける。子プロセスが残っていることを理由に環境を維持しない。

### REQ-062: 残存プロセスへの有限の終了猶予

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A67
- verification: unit

主コマンド終了時は公開と外部通信を先に止め、残った子プロセスに終了を要求する。有限の猶予を与え、猶予後も残るプロセスは強制終了する。

### REQ-063: 終了猶予の既定値と共通期限

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A68
- verification: unit

終了猶予は既定5秒とし、設定で1〜300秒の整数に変更できる。残ったプロセス全体で共通の期限を使い、全員が終了したら期限を待たずに片付ける。

## Examples

```gherkin
@id=EX-112 @about=REQ-060 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A65
Scenario: 終了した環境のネットワーク資源を回収する
  Given 隔離環境にIP許可と公開ポートと補助プロセスがある
  When その隔離環境が終了する
  Then その環境のIP許可と公開ポートと補助プロセスを片付ける

@id=EX-113 @about=REQ-060 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A65
Scenario: 別の環境の公開を終了させない
  Given 独立した隔離環境XとYがそれぞれサービスを公開している
  When 隔離環境Xが終了する
  Then 隔離環境Yの通信とサービス公開を維持する

@id=EX-114 @about=REQ-061 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A66
Scenario: 主コマンド終了後にレビューサービスだけを残さない
  Given 主コマンドが起動したレビューサービスが隔離環境内で動作し公開されている
  When 主コマンドが終了する
  Then レビューサービスを含む残った子プロセスも終了する
  And 隔離環境を終了して公開ポートを片付ける

@id=EX-115 @about=REQ-062 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A67
Scenario: 通信を停止してから終了処理の機会を与える
  Given 主コマンドが終了し子プロセスが残っている
  When 隔離環境の終了処理を始める
  Then 公開と外部通信を先に止める
  And 子プロセスに終了を要求して有限の猶予を与える

@id=EX-116 @about=REQ-062 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A67
Scenario: 終了要求に応じない子プロセスを待ち続けない
  Given 残った子プロセスに終了を要求して猶予を与えている
  When 猶予が終了してもそのプロセスが残っている
  Then そのプロセスを強制終了する

@id=EX-117 @about=REQ-063 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A68
Scenario: 複数プロセスが残っても猶予を人数分延ばさない
  Given 終了猶予を設定しておらず子プロセスが3つ残っている
  When 終了猶予を与える
  Then 全体で共通の5秒の期限を使う

@id=EX-118 @about=REQ-063 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A68
Scenario: 全員が終了したら猶予を使い切らない
  Given 終了猶予を300秒に設定している
  When 猶予開始から1秒で残った子プロセスがすべて終了する
  Then 残り299秒を待たずに片付ける

@id=EX-119 @about=REQ-063 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A68
Scenario: 範囲外の猶予を受け付けない
  Given 終了猶予に301秒が指定されている
  When 設定を検査する
  Then 設定エラーにする
```
