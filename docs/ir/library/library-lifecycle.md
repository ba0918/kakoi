# 組み込み実行の所有権と寿命

起動後の監督、停止要求、所有者の消失、並行実行、非同期待機と入れ子実行を定める未承認草案。

## Requirements

### REQ-library-201: 利用者の待機から独立した監督
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A6
- verification: unit

起動後は`実行ハンドル`を返し、利用者の待機やイベント受信に依存せず監督を継続する。
呼び出し元プロセス全体のシグナル設定を暗黙に変更しない。

### REQ-library-202: 停止要求と回収確認
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A6, docs/decision/brainstorm/2026-10-03-public-library-api.md#A8
- verification: unit

停止要求の受付と終了および回収の完了を区別する。
明示的な待機により終了結果を取得でき、停止を要求しただけでは回収完了を報告しない。

### REQ-library-203: 所有者を失った実行の終了
- kind: event_driven
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A8
- verification: unit

`実行ハンドル`の破棄は停止処理を開始する。
呼び出し元が異常終了した場合も監督側が検知して隔離を終了する。
所有者を失った実行の継続を選ぶAPIは提供しない。

### REQ-library-204: 複数実行の独立性
- kind: invariant
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A9
- verification: unit

同一プロセスから複数の隔離を並行起動でき、一方の停止は他方を終了させない。
同一ホスト側公開ポートの要求による資源競合は起動エラーにする。

### REQ-library-205: 同期と非同期の待機
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A17, docs/decision/brainstorm/2026-10-03-public-library-api.md#A27
- verification: unit

終了とイベントの待機は同期版と、特定ランタイムへの依存を必須にしないFutureを返す非同期版を提供する。
待機Futureのキャンセルは待機だけをやめ、`実行ハンドル`が残っていれば実行を継続する。
イベント待機がPendingのままキャンセルされた場合は未取得イベントを消費せず、保持上限による欠落がなければ次の待機で取得できる。
ハンドルの破棄はREQ-library-203に従う。
計画と起動は同期APIとする。

### REQ-library-206: 入れ子でも要求した隔離を作る
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A18
- verification: unit

組み込みAPIは既存の隔離内でも要求された新しい隔離を作り、外側の制限も引き続き適用する。
作れなければコマンドを起動せずエラーにし、隔離を暗黙に省略しない。
CLIの既定の入れ子exec動作は維持する。

## Examples

```gherkin
@id=EX-library-201 @about=REQ-library-201 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A6
Scenario: 利用者が別の処理をしていても監督する
  Given 起動した隔離のハンドルを利用者が保持している
  When 利用者が待機もイベント受信も行わず別の処理をする
  Then 隔離の監督は継続する

@id=EX-library-202 @about=REQ-library-201 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A6
Scenario: シグナル設定を奪わない
  Given 呼び出し元が自身のシグナル処理を設定している
  When 組み込みAPIで隔離を起動して終了する
  Then 呼び出し元のシグナル設定を暗黙に置き換えない

@id=EX-library-203 @about=REQ-library-202 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A6,docs/decision/brainstorm/2026-10-03-public-library-api.md#A8
Scenario: 停止を要求してから回収結果を待つ
  Given コマンドと子プロセスが隔離内で動いている
  When 利用者が停止を要求して明示的に待機する
  Then 要求受付と終了結果を区別して取得できる

@id=EX-library-204 @about=REQ-library-202 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A6
Scenario: 停止要求を回収完了と取り違えない
  Given 停止要求が受け付けられたが回収はまだ終わっていない
  When 利用者が終了状態を照会する
  Then 停止要求の受付だけを根拠に回収完了と報告しない

@id=EX-library-205 @about=REQ-library-203 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A8
Scenario: ハンドルの破棄で停止する
  Given コマンドが隔離内で動いている
  When 利用者が実行ハンドルを破棄する
  Then 停止処理を開始する

@id=EX-library-206 @about=REQ-library-203 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A8
Scenario: 呼び出し元の強制終了でも残し続けない
  Given 隔離内でコマンドが動いている
  When 呼び出し元が強制終了されてハンドルの破棄処理も走らない
  Then 監督側が所有者の死亡を検知して隔離を終了する

@id=EX-library-207 @about=REQ-library-204 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A9
Scenario: 一方だけを停止する
  Given 同一プロセスが2つの隔離を並行起動している
  When 一方を停止する
  Then 他方の実行は継続する

@id=EX-library-208 @about=REQ-library-204 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A9
Scenario: 公開ポートの競合を拒む
  Given 動いている隔離がホスト側の公開ポートを使用している
  When 別の隔離が同じ公開ポートを要求する
  Then 資源競合として起動エラーを返す

@id=EX-library-209 @about=REQ-library-205 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A17
Scenario: 非同期に終了を待つ
  Given 非同期アプリが実行ハンドルを保持している
  When 終了待機のFutureを待つ
  Then 同期版と同じ終了結果を取得できる

@id=EX-library-210 @about=REQ-library-205 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A17
Scenario: 待機のキャンセルだけでは停止しない
  Given 実行ハンドルを保持したまま終了またはイベントのFutureを待っている
  When 待機Futureだけを破棄する
  Then 隔離の実行は継続する
  And 再び待機できる

@id=EX-library-213 @about=REQ-library-205 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A27
Scenario: キャンセル後にイベントを取り直す
  Given イベント待機がPendingで、履歴は保持上限を超えていない
  When その待機をキャンセルしてからイベントが届き、再び待機する
  Then キャンセルを原因にイベントを失わず取得できる

@id=EX-library-214 @about=REQ-library-205 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A27
Scenario: キャンセル中に保持上限を超えた
  Given イベント待機をキャンセルし、受信を再開するまでに保持上限を超える
  When イベントの待機を再開する
  Then 失われた履歴を取りこぼしとして検知できる

@id=EX-library-211 @about=REQ-library-206 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A18
Scenario: 入れ子でもポリシーを適用する
  Given 既存の隔離内から組み込みAPIを呼ぶ
  When 要求された新しい隔離を作れる
  Then 新しい隔離を作ってコマンドを実行し、外側の制限も維持する

@id=EX-library-212 @about=REQ-library-206 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A18
Scenario: 入れ子を作れないときに直接実行しない
  Given 既存の隔離の制限により要求された新しい隔離を作れない
  When 組み込みAPIで起動を要求する
  Then コマンドを起動せずエラーを返す
```
