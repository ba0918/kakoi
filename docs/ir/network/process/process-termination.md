# 外部からの終了要求

filteredへのSIGTERMによる終了要求を定義する草案。猶予の起点はprocess-shutdown-deadline.mdで定義する。安全上の故障が重なる場合はprocess-safety-exit-priority.mdを優先する。停止機構とイベント順序の実証は未決。

## 要求

### REQ-101: SIGTERMによる環境全体の終了

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A106
- 検証: unit

"filtered" で外部からkakoiへSIGTERMによる終了要求が届いたら、隔離環境全体の終了処理に入る。通信・公開を止め、主コマンドと子プロセスに終了を要求し、設定済みの猶予後も残るプロセスは強制終了する。

### REQ-102: 実行中のSIGTERMによる終了結果

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A107,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A140
- 検証: unit

主コマンドの実行中にSIGTERMを受けて環境の終了処理を始めた場合、主コマンドが後処理後に0を返しても終了コード143を返す。ただし安全上の故障による125優先規則に該当する場合はそちらを優先する。

### REQ-103: 重複するSIGTERMで猶予を変更しない

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A108
- 検証: unit

SIGTERMで開始した終了猶予中に再度SIGTERMが来ても、最初の猶予期限を維持し、延長も短縮もしない。主コマンド終了後のCtrl+Cによる打切りは引き続き行う。

### REQ-104: 主コマンド終了後のSIGTERMで結果と期限を変えない

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A109
- 検証: unit

主コマンドの終了を確認して子プロセスの終了猶予に入った後にSIGTERMが来ても、確定済みの主コマンドの結果と既存の猶予期限を維持する。

## 具体例

```gherkin
@id=EX-219 @about=REQ-101 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A106
Scenario: 外部の終了要求で環境全体の片付けを始める
  Given filteredで主コマンドと子プロセスが動作している
  When 外部からkakoiへSIGTERMが届く
  Then 通信と公開を止める
  And 主コマンドと子プロセスに終了を要求する

@id=EX-220 @about=REQ-101 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A106
Scenario: 終了要求に応じない主コマンドも猶予後に止める
  Given 外部のSIGTERMにより終了処理を開始している
  And 主コマンドが終了要求後も動き続けている
  When 設定済みの終了猶予が過ぎる
  Then 残った主コマンドと子プロセスを強制終了する

@id=EX-221 @about=REQ-102 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A107
Scenario: 終了要求後の後処理が成功しても中断の結果を返す
  Given filteredで主コマンド実行中にSIGTERMを受け終了処理を開始した
  And 安全上の故障による125優先規則には該当しない
  When 主コマンドが後処理を終えて0で終了する
  Then kakoiは終了コード143を返す

@id=EX-222 @about=REQ-102 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A107
Scenario: 猶予後の強制終了でも外部終了要求の結果を返す
  Given filteredで主コマンド実行中にSIGTERMを受け終了処理を開始した
  And 安全上の故障による125優先規則には該当しない
  When 主コマンドが猶予内に終了せず強制終了する
  Then kakoiは終了コード143を返す

@id=EX-223 @about=REQ-103 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A108
Scenario: 再度のSIGTERMでも残り時間を維持する
  Given SIGTERMで開始した終了猶予の期限まで3秒残っている
  When 再度SIGTERMが届く
  Then 猶予を再開始せず残り3秒の期限を維持する
  And 再度のSIGTERMだけを理由に直ちに強制終了しない

@id=EX-224 @about=REQ-103 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A108,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A106
Scenario: 終了要求が重なり続けても期限を延ばさない
  Given SIGTERMで開始した終了猶予中に複数回SIGTERMを受けている
  And 残ったプロセスがまだ終了していない
  When 最初に設定した猶予期限に達する
  Then 残ったプロセスを強制終了する

@id=EX-225 @about=REQ-104 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A109
Scenario: 主コマンド完了後のSIGTERMで成功を中断扱いにしない
  Given 主コマンドの終了コード0を確認し子プロセスの終了猶予に入っている
  And その猶予は残り3秒である
  When SIGTERMが届く
  Then 残り3秒の期限を維持する
  And 主コマンドの終了コード0を維持する

@id=EX-226 @about=REQ-104 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A109
Scenario: 主コマンドの失敗結果も片付け中のSIGTERMで変えない
  Given 主コマンドの終了コード7を確認し子プロセスの終了猶予に入っている
  When SIGTERMが届く
  Then 猶予期限を変更しない
  And 主コマンドの終了コード7を維持する
```
