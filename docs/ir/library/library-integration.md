# 公開APIの利用と配布

利用者向けの入口、既存機能との関係、補助処理の供給と段階的な配布を定める未承認草案。

## Requirements

### REQ-library-401: 利用者向けの入口
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A15
- verification: unit

正式な利用者向け入口はruntimeとpolicyに絞る。
基本利用はruntimeへの依存追加で済み、必要なポリシー型をruntimeから再公開する。
ポリシーのみを扱う利用者はpolicyを直接利用できる。
内部クレートの細かな操作を基本利用の必須手順にしない。

### REQ-library-402: 既存機能とCLIの維持
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A3, docs/decision/brainstorm/2026-10-03-public-library-api.md#A4, docs/decision/brainstorm/2026-10-03-public-library-api.md#A18
- verification: unit

公開APIはhost、none、filtered、listed、コマンドガードを含む既存機能を扱う。
CLIと組み込み先は実行処理を共用し、既存CLIの挙動と設定形式を維持する。
組み込みAPIの入れ子実行はREQ-library-206に従う。
現在の未安定なRust APIの互換性は要求しない。

### REQ-library-403: 同一バイナリによる補助処理
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A7
- verification: unit

補助処理は利用側バイナリの再実行を基本方式とする。
利用側はmainの先頭でアプリと非同期ランタイムの初期化より前に公式の振り分け関数を呼ぶ。
別helperの配置とパス指定を基本利用の必須手順にしない。

### REQ-library-404: path依存での実験利用
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A2, docs/decision/brainstorm/2026-10-03-public-library-api.md#A15
- verification: unit

初期段階は外部パッケージからpath依存で実験利用できるAPIと実例を提供する。
実例は正式な利用者向け入口だけを使う。

### REQ-library-405: crates.ioでの配布
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A2, docs/decision/brainstorm/2026-10-03-public-library-api.md#A15
- verification: review
- how_to_verify: 次段階の公開時にcrates.io上のパッケージと依存情報を確認し、runtimeとpolicyおよび依存解決に必要な内部クレートが取得できることを確認する。
- deferred: docs/decision/brainstorm/2026-10-03-public-library-api.md#A2

path依存による実験利用の次段階でcrates.ioへ公開し、依存解決に必要な内部クレートも配布する。

### REQ-library-406: 補助処理の動的リンク依存
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A21
- verification: unit

自己再実行に必要な動的リンカと共有ライブラリをポリシー外から自動公開しない。
補助処理を動かせない場合は対象コマンドを起動する前にエラーにする。
利用側は必要なファイルを明示的に公開するか静的リンクを選び、静的リンク自体は必須にしない。
依存ファイルが既に見える構成では追加設定を要求しない。

### REQ-library-407: 記述子マウント機能
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A22
- verification: unit

新しい組み込みAPIはbwrapの "--bind-fd" と "--ro-bind-fd" を必須とし、版名だけに頼らず機能を確認する。
不足時は起動エラーにし、既存CLIの従来の対応条件は維持する。

### REQ-library-408: ガードの役割を実行イメージに保持する
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A24
- verification: unit

組み込みAPIは読み取り可能な自己実行ファイルを要求する。
初期版は役割識別情報を付けた利用側バイナリを、bwrapのro-bind-dataにより配置先ごとに読み取り専用の実行ファイルとしてコピーする。
同一inodeの共有は要求しない。
設定表を失ったガードは通常アプリの処理に戻らず、終了126とする。
輸送用memfd自体の実行は要求しない。
配置した実行ファイルを実行できなければ起動エラーにし、ディスク退避とOS設定変更を自動実行しない。
利用条件には、バイナリサイズと配置数に依存するコピーコストと、main前の初期化処理も再実行されることを明記する。
公開APIはガードの配置方式を契約に含めず、将来の共有化に公開APIの変更を要求しない。
既存CLIの従来経路は維持する。

### REQ-library-409: 共有ファイルの作成規則
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A25
- verification: unit

組み込みAPIでもREQ-305とREQ-461の共有ファイルの作成規則を共用する。
場所は要求に渡された環境から決め、非入れ子の起動で既存規則に従って空ファイルと固定内容のresolverファイルを準備する。
ガード用実行ファイルは共有ファイルの置き場へ保存しない。

### REQ-library-410: 利用側アプリ自身へのガード
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A26
- verification: unit

組み込みAPIは利用側アプリ自身がガード対象として指定された場合も規則を適用し、自己再実行するバイナリであることを理由に除外しない。
CLIのkakoi自身に対する既存の除外は維持する。

## Examples

```gherkin
@id=EX-library-401 @about=REQ-library-401 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A15
Scenario: runtimeだけを直接依存に加える
  Given 利用側パッケージがruntimeを直接依存に指定する
  When 公開されたポリシー型で要求を構築して起動する
  Then 内部クレートを直接依存へ加えずに利用できる

@id=EX-library-402 @about=REQ-library-401 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A15
Scenario: ポリシーだけを扱う
  Given 利用側パッケージがpolicyだけを直接依存に指定する
  When ポリシーを構築して検証する
  Then runtimeの起動処理を呼ばずに利用できる

@id=EX-library-403 @about=REQ-library-402 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A3,docs/decision/brainstorm/2026-10-03-public-library-api.md#A4
Scenario: 既存CLIの挙動を維持する
  Given 既存の設定とCLIの起動方法がある
  When CLIが共有した実行処理を使う
  Then CLIの挙動と設定形式は維持される

@id=EX-library-404 @about=REQ-library-402 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A3
Scenario: 組み込みでもガードを適用する
  Given 組み込み側のポリシーにコマンドガードがある
  When ガードが禁止する引数でコマンドを実行する
  Then ライブラリ利用を理由にガードを省略しない

@id=EX-library-419 @about=REQ-library-402 @source=docs/decision/brainstorm/2026-10-03-library-device-nodev.md#A1
Scenario: ポリシーが示すデバイスはCLIと同じく開けない
  Given ポリシーのroまたはrw-fileが文字デバイスを示す
  When 組み込みAPIで計画し起動する
  Then 隔離内でそのデバイスは見えるが、読み込みでも書込みでも開けない

@id=EX-library-405 @about=REQ-library-403 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A7
Scenario: 単一バイナリから補助処理を起動する
  Given 利用側のmainの先頭が公式の振り分け関数を呼ぶ
  When 補助処理のために同じバイナリを再実行する
  Then アプリ初期化より前に補助処理へ振り分ける
  And 別helperの配置とパス指定は要らない

@id=EX-library-406 @about=REQ-library-404 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A2,docs/decision/brainstorm/2026-10-03-public-library-api.md#A15
Scenario: 外部パッケージの実例を動かす
  Given workspace外の利用側パッケージがpath依存を指定する
  When 公開APIだけを使う実例をビルドして実行する
  Then 内部モジュールを直接呼ばずに隔離を利用できる

@id=EX-library-407 @about=REQ-library-405 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A2,docs/decision/brainstorm/2026-10-03-public-library-api.md#A15
Scenario: 次段階でレジストリから取得する
  Given crates.ioへの公開段階に進んでいる
  When 利用側が公開されたruntimeに依存する
  Then 依存解決に必要な内部クレートもcrates.ioから取得できる

@id=EX-library-408 @about=REQ-library-406 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A21
Scenario: 動的リンクの依存が見える構成で起動する
  Given ポリシーが自己再実行に必要な動的リンカと共有ライブラリを既に見せている
  When 動的リンクした利用側バイナリから隔離を起動する
  Then 静的リンクと追加の公開設定を要求しない

@id=EX-library-409 @about=REQ-library-406 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A21
Scenario: 補助処理の依存不足を自動公開で補わない
  Given listedの設定で自己再実行に必要な共有ライブラリが見えない
  When 隔離の起動を要求する
  Then 対象コマンドを起動する前にエラーを返す
  And ポリシー外のファイルを自動公開しない

@id=EX-library-410 @about=REQ-library-407 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A22
Scenario: 版名だけで機能を決めない
  Given bwrapに記述子マウント機能がある
  When 組み込みAPIが起動条件を調べる
  Then 版名だけでなく機能の有無で判断する

@id=EX-library-411 @about=REQ-library-407 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A22
Scenario: 記述子マウント機能がない
  Given bwrapに必要な記述子マウント機能がない
  When 新しい組み込みAPIで起動する
  Then 起動エラーを返す

@id=EX-library-412 @about=REQ-library-408 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A24
Scenario: 複数のガードを配置先ごとにコピーする
  Given 1つの隔離に複数のコマンドガードを置く
  When 起動の準備を行う
  Then 各配置先に役割識別情報付きの読み取り専用の実行ファイルを配置する
  And 同一inodeの共有を起動の条件にしない

@id=EX-library-413 @about=REQ-library-408 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A24
Scenario: 表を失っても通常アプリへ戻らない
  Given ガード用の実行イメージが起動されるが設定表を読めない
  When 公式の振り分け処理を行う
  Then 通常アプリの処理へ戻らず終了126とする

@id=EX-library-414 @about=REQ-library-408 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A24
Scenario: 配置したガードの実行が拒否される
  Given ガードを使う要求があり、配置した実行ファイルの実行が環境の制限により拒否される
  When 起動を要求する
  Then エラーにし、ディスク退避とOS設定変更を自動実行しない

@id=EX-library-415 @about=REQ-library-409 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A25
Scenario: 要求の環境が指定する共有ファイルの置き場を使う
  Given 呼び出し元とは異なるXDG_RUNTIME_DIRを要求に渡し、非入れ子で起動する
  When 既存規則に従って共有ファイルを準備する
  Then 要求が指定した場所に空ファイルと固定内容のresolverファイルを準備する

@id=EX-library-416 @about=REQ-library-409 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A25
Scenario: ガードのコピーをホストへ保存しない
  Given 共有ファイルの置き場を使い、複数のガードを配置する
  When 起動の準備を行う
  Then ガード用実行ファイルを共有ファイルの置き場へ保存しない

@id=EX-library-417 @about=REQ-library-410 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A26
Scenario: 利用側アプリ自身の禁止引数を止める
  Given 利用側アプリ自身をガード対象とする規則がある
  When 組み込みAPIでそのアプリを禁止された引数で実行する
  Then 自己再実行用バイナリという理由で規則を省略せず、禁止として終了する

@id=EX-library-418 @about=REQ-library-410 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A26
Scenario: CLIの自己除外は維持する
  Given CLIの規則がkakoi自身を対象にする
  When CLIがガードを計画する
  Then 既存の自己除外を維持する
```
