# 実行結果と入出力

終了結果、イベントの保持、標準入出力と非同期I/Oへの接続を定める未承認草案。

## Requirements

### REQ-library-301: 終了結果の区別
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A11
- verification: unit

終了結果は主コマンドの結果、終了理由、通信遮断とプロセス回収の確認状況を区別する。
コマンド成功と回収未確認を単なる成功に集約しない。
通信遮断の保証はfilteredに適用し、hostに同じ保証を与えない。
CLIは結果を従来の終了コードへ変換する。

### REQ-library-302: 遅い受信者とイベントの保持
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A12
- verification: unit

イベントは構造化し、履歴の保持量を無制限に増やさず、取りこぼしを検知できる方式とする。
現在状態と最終結果は履歴とは別に取得でき、受信の遅延は監督と停止処理を止めない。

### REQ-library-303: 標準入出力の指定
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A14
- verification: unit

組み込みAPIの標準入出力は、継承、パイプ、破棄、呼び出し側が用意したファイルまたは記述子の指定に対応する。
Rust標準のStdioに近い指定方法を用い、組み込みAPIでは指定を明示させ、CLIの端末継承は維持する。
パイプの読み書きは利用側が担当し、コマンドの入出力と状態イベントを別経路にする。
出力の未読によってコマンドがパイプ待ちになっても、監督処理を停止しない。

### REQ-library-304: 利用側の非同期I/Oへの接続
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A20, docs/decision/brainstorm/2026-10-03-public-library-api.md#R1
- verification: unit

所有権付きのパイプまたは記述子を利用側へ渡し、利用側が非同期ランタイムの型へ変換できる。
公式の接続例で所有権の移動と非ブロッキング設定を確認する。
初期APIは独自の非同期ストリーム抽象と新規疑似端末の作成管理を提供しない。
既存端末の継承と、呼び出し側が用意した端末への接続は提供する。

## Examples

```gherkin
@id=EX-library-301 @about=REQ-library-301 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A11
Scenario: コマンド成功と後処理を別々に確認する
  Given 主コマンドが終了コード0で終了し、通信遮断と回収も確認できた
  When 終了結果を取得する
  Then コマンドの成功と後処理の確認状況を別々に取得できる

@id=EX-library-302 @about=REQ-library-301 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A11
Scenario: コマンド成功だけで回収成功にしない
  Given 主コマンドの成功を確認した後、制御障害で回収を確認できなくなる
  When 終了結果を取得する
  Then コマンドの成功と回収未確認を区別し、単なる成功に集約しない

@id=EX-library-303 @about=REQ-library-301 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A11
Scenario: hostで通信遮断を保証しない
  Given hostモードのコマンドが終了した
  When 終了結果を取得する
  Then filteredと同じ通信遮断の保証を報告しない

@id=EX-library-304 @about=REQ-library-302 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A12
Scenario: イベントを読まずに停止できる
  Given 利用側がイベントを受信していない
  When 隔離の停止を要求する
  Then 受信を待たずに停止処理を進め、現在状態と最終結果を取得できる

@id=EX-library-305 @about=REQ-library-302 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A12
Scenario: 保持できなかった履歴を識別する
  Given 利用側の受信が遅れ、イベントの保持量を超える
  When 利用側が受信を再開する
  Then 履歴の取りこぼしを検知できる
  And 履歴の保持量を無制限に増やさない

@id=EX-library-306 @about=REQ-library-303 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A14
Scenario: 指定した標準入出力を使う
  Given 利用側が継承、パイプ、破棄、用意済みのファイルまたは記述子を明示する
  When コマンドを起動する
  Then 指定した標準入出力を使い、状態イベントは別経路で取得できる

@id=EX-library-307 @about=REQ-library-303 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A14
Scenario: 未読のコマンド出力が停止処理を妨げない
  Given パイプへ大量出力するコマンドを利用側が読んでいない
  When 利用側が停止を要求する
  Then コマンドのパイプ待ちによって監督処理は停止しない

@id=EX-library-308 @about=REQ-library-304 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A20
Scenario: パイプを非同期I/Oへ渡す
  Given 利用側が所有権付きのパイプを受け取る
  When 公式例に従って非ブロッキング設定と非同期I/O型への変換を行う
  Then 非同期に読み書きでき、元の所有者との二重解放が起きない

@id=EX-library-309 @about=REQ-library-304 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#R1
Scenario: 端末の作成は外部が担当する
  Given 呼び出し側が疑似端末を用意している
  When その記述子を組み込みAPIに渡す
  Then kakoiによる新規疑似端末の作成を要求せずに接続できる
```
