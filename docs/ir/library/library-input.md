# 組み込みAPIの入力と計画

メモリ上のポリシー入力、明示的な実行環境、不変な計画と起動直前の検査を定める未承認草案。

## Requirements

### REQ-library-101: 明示的なポリシー入力
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A5
- verification: unit

組み込み用の主入口はメモリ上のポリシーを受け取り、ホームの既存設定を暗黙に読み込まない。
設定ファイルの探索、読み込み、合成は明示的に呼ぶ別入口とし、CLIも共通の検証と計画処理を使う。

### REQ-library-102: RustとTOMLの共通検証
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A13
- verification: unit

ポリシーはRustの型とメモリ上のTOMLの両方から構築でき、どちらも共通の検証を通す。
Rustからの利用にTOML文字列の生成や一時ファイルの作成を要求しない。

### REQ-library-103: モードの明示
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A13
- verification: unit

新しいRust構築APIはマウント、ネットワーク、環境の各モードを明示させる。
既存TOMLでこれらを省略した場合の意味は維持する。

### REQ-library-104: 要求ごとの実行環境
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A9, docs/decision/brainstorm/2026-10-03-public-library-api.md#D13, docs/decision/brainstorm/2026-10-03-public-library-api.md#A29
- verification: unit

組み込みAPIは起動ごとに環境と作業ディレクトリを受け取り、呼び出し元のグローバルな状態を書き換えずに適用する。
相対パスは要求の作業ディレクトリを基準に解決する。
名前で渡した対象コマンドは、要求の環境にポリシーを適用した隔離用の実効PATHで解決する。
"env.set" の "PATH" と "path-prepend" を反映し、ガードの本物選びと配置・省略はREQ-446とREQ-449に従い、対象コマンドの名前探索にもその配置を反映する。
ホスト側の "bwrap" は、ポリシー適用前の要求の "HostContext" の "PATH" から探す。

### REQ-library-105: 不変で一度だけ起動できる計画
- kind: invariant
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A10
- verification: unit

`実行用計画`は生成完了後に利用者が内部を書き換えられず、読み取り用の説明を提供する。
起動は計画を消費し、同じ計画を繰り返し起動するAPIを提供しない。
条件を変更する場合は新しい要求から計画を作る。

### REQ-library-106: 成立条件を失った計画の拒否
- kind: event_driven
- source: docs/decision/brainstorm/2026-10-03-public-library-api.md#A19, docs/decision/brainstorm/2026-10-03-public-library-api.md#A10
- verification: unit

起動直前の検査で計画の成立条件を満たさないと判明した場合は、コマンドを起動せず再計画を必要とするエラーを返す。
マウント元のリンク先の変化を検出したときは別の対象を黙って公開せず、自動的に許可範囲を変更しない。
通常のマウント先のファイル内容を固定する保証は設けない。

## Examples

```gherkin
@id=EX-library-101 @about=REQ-library-101 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A5
Scenario: メモリ入力は既存profileを合成しない
  Given ホームに既存profileがあり、呼び出し側が別のポリシーをメモリ上で渡す
  When 組み込み用の主入口で計画する
  Then 渡したポリシーを使い、既存profileを暗黙に合成しない

@id=EX-library-102 @about=REQ-library-101 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A5
Scenario: ファイル入力は明示して選ぶ
  Given 呼び出し側が設定ファイルの読み込み入口を明示的に呼ぶ
  When 読み込んだポリシーを計画する
  Then メモリ入力と共通の検証と計画処理を通る

@id=EX-library-103 @about=REQ-library-102 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A13
Scenario: Rust型から一時ファイルなしで構築する
  Given 呼び出し側がRustの型で有効なポリシーを構築する
  When ポリシーを検証する
  Then TOML文字列や一時ファイルを作らずに利用できる

@id=EX-library-104 @about=REQ-library-102 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A13
Scenario: 入力経路を変えても不正な条件を通さない
  Given 同じ不正なポリシー条件をRustの型とTOMLで指定する
  When 両方を検証する
  Then どちらもその条件を拒否する

@id=EX-library-105 @about=REQ-library-103 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A13
Scenario: Rust構築ではモードの省略を認めない
  Given 新しいRust構築APIでマウント、ネットワーク、環境のいずれかのモードを指定しない
  When 実行に使うポリシーを構築しようとする
  Then モードを暗黙に補って実行可能なポリシーにしない

@id=EX-library-106 @about=REQ-library-103 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A13
Scenario: 既存TOMLの省略を維持する
  Given 各モードを省略した既存TOMLがある
  When 新しい入口からそのTOMLを読み込む
  Then 省略時の意味は既存CLIと同じになる

@id=EX-library-107 @about=REQ-library-104 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A9,docs/decision/brainstorm/2026-10-03-public-library-api.md#D13
Scenario: 呼び出し元と異なる作業ディレクトリを使う
  Given 要求の作業ディレクトリと環境が呼び出し元のものと異なる
  When 要求内の相対パスを解決してコマンドを起動する
  Then 要求の作業ディレクトリと環境を使う
  And 呼び出し元の作業ディレクトリと環境は変わらない

@id=EX-library-108 @about=REQ-library-105 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A10
Scenario: 計画の説明を読んで起動する
  Given 検証済みの実行用計画がある
  When 利用者が説明を確認して起動する
  Then 起動はその計画を消費する

@id=EX-library-109 @about=REQ-library-105 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A10
Scenario: 計画の書換えと再利用を拒む
  Given 利用者が公開APIから生成済み計画の内部書換えか起動済み計画の再利用を試す
  When 利用側パッケージをコンパイルする
  Then コンパイルで拒否される

@id=EX-library-110 @about=REQ-library-106 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A19
Scenario: 起動前にリンク先が変わった
  Given 計画後にマウント元のシンボリックリンクが別の対象へ変更される
  When 起動直前の検査が変更を検出する
  Then 別の対象を公開せず再計画を必要とするエラーを返す

@id=EX-library-111 @about=REQ-library-106 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A19
Scenario: 通常のマウントは内容のスナップショットではない
  Given 通常のマウント先にファイルがある
  When ホストからそのファイルの内容を変更する
  Then 計画の不変性を理由に古い内容を固定する保証はない

@id=EX-library-112 @about=REQ-library-104,REQ-446,REQ-449 @source=docs/decision/brainstorm/2026-10-03-public-library-api.md#A29
Scenario: 対象コマンドとホスト側bwrapのPATHを区別する
  Given 要求の "HostContext" の "PATH" にある "tool" と、ポリシー適用後の隔離用PATHにある同名の "tool" は異なる実行可能な通常ファイルで、どちらも隔離内に見える
  And "env.set" の "PATH" と "path-prepend" により、後者がガードの場所を除いた隔離用PATHで最初の候補になる
  And "tool" のガード規則が設定され、REQ-446とREQ-449に従ってガードが配置される
  And ホスト側の "bwrap" は要求の "HostContext" の "PATH" にある
  When "tool" を名前で渡して計画し起動する
  Then 対象コマンドは隔離用の実効PATHに配置されたガードを通り、そのガードの本物はポリシー適用後の "tool" になる
  And ホスト側の "bwrap" はポリシー適用前の要求の "HostContext" の "PATH" から選ばれる
```
