# 入れ子と並列起動

既存仕様から継承した本体の検査用表現。同じ責務を一文書にまとめ、見出しと要求IDで参照する。この IR が正本である。

## Requirements

### REQ-284: 入れ子の実行
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A1, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A10, docs/decision/brainstorm/2026-10-03-public-library-api.md#D11
- verification: unit

入れ子（REQ-455の判定）で"--nested=exec"の実行（"--nested"を省略した実行を含む）は、ポリシー読込・bwrap起動・環境変更・cwdとHOMEの検査を行わず、内側プロファイルを適用しないwarningを1行出してコマンドを直接execする。initと計画表示はこの規則の対象外。"--nested=isolate"の実行はREQ-456に従う。

前段の入れ子オプションと直接execによる隔離の省略はCLIに限る。
組み込みAPIはREQ-library-206に従い、入れ子でも要求された新しい隔離を作り、作れなければコマンドを起動せずエラーにする。

### REQ-285: 入れ子の計画
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A8, docs/decision/brainstorm/2026-10-03-public-library-api.md#D11
- verification: unit

"--nested=exec"の入れ子の計画表示はポリシーを読み、計画に入れ子の表示と、その計画は使われないことを付ける。計画のPATHは隔離環境の値、解決先はホストPATHによる値として区別する。

前段の入れ子オプションに基づく、使われない計画の表示はCLIに限る。
組み込みAPIはREQ-library-206に従って新しい隔離を計画し、入れ子であることを理由に隔離を省略しない。

### REQ-286: 入れ子の判定の限界
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-29-nested-isolation.md#A25, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A27, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A19, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A20, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A13, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A16
- verification: review
- how_to_verify: READMEから直接リンクする "docs/security.md" の既知の隙間に、本文の限界が書かれていることを確かめる。

隔離の中のプロセスが自分で名前空間を作って "/dev" を覆うと、その中で起動したkakoiからは入れ子の印が見えず、入れ子でないと判定される。入れ子の印を置かない古い版のkakoiの隔離の中では、新しい版のkakoiは入れ子と判定しない。"--nested=isolate"の入れ子（2段目）の中でさらに"--nested=isolate"で隔離（3段目）を作るとき、3段目が、2段目がデータから作って置いたファイル（"hide"のファイル、filteredの"/etc/resolv.conf"）と同じパスにマウントしようとすると、コマンドを実行せずに止まる。外の起動が共有ファイルの置き場を使えなかったときの2段目も同じである。この4つをREADMEに記す。

### REQ-287: 並列起動
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A49
- verification: unit

コマンドを包む並列起動は内部状態を共有せず（共有ファイルの置き場は中身が決まったファイルだけを置くので、起動の結果を互いに左右しない）、一方の終了で他方を停止しない。同じNAMEのinitの競合結果は保証しない。filteredの公開ポート競合は既存の公開割当・復帰仕様に従う。

### REQ-288: 診断と警告の形
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-10-03-public-library-api.md#D11
- verification: unit

診断は標準エラー1行のkakoi: <種類>: <説明>、警告はkakoi: warning: <説明>とする。種類は契約、説明は自由文。警告は終了結果に先行して出ることがある。

前段の標準エラーへの診断と警告の表示形式はCLIとガード子プロセスの出力の契約とする。
組み込みAPIの制御エラーと状態通知はREQ-library-301とREQ-library-302に従って構造化して返す。
ガード子プロセスの標準エラーをすべて状態イベントへ変換することは要求しない。

## Examples

```gherkin
@id=EX-520 @about=REQ-284 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1,docs/decision/brainstorm/2026-09-29-nested-isolation.md#A10,docs/decision/brainstorm/2026-09-29-nested-isolation.md#A23
Scenario: 入れ子の実行
  Given 入れ子の印がある隔離の中にいる
  When "--nested"を付けずにコマンドを実行する
  Then 受け取った環境のまま直接execしてwarningを出す

@id=EX-521 @about=REQ-285 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1,docs/decision/brainstorm/2026-09-29-nested-isolation.md#A23
Scenario: 入れ子の計画
  Given 入れ子の印がある隔離の中にいる
  When 存在しない名前付きプロファイルの計画を見る
  Then 計画を出さずpolicy診断となる

@id=EX-522 @about=REQ-286 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A19
Scenario: 3段目の入れ子の隔離は止まる
  Given "--nested=isolate"で作った入れ子の隔離の中にいて、その隔離がファイルを"hide"にしている
  When 同じファイルを"hide"にするポリシーと"--nested=isolate"でコマンドを起動する
  Then コマンドを実行せずに止まる

@id=EX-523 @about=REQ-287 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 並列起動
  Given 本体の既存仕様を適用する
  When 2個の独立した起動が終了コード5と6を返す
  Then 各呼出元へそれぞれ5と6を返す

@id=EX-524 @about=REQ-288 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 診断と警告の形
  Given 本体の既存仕様を適用する
  When policy診断を出す
  Then 標準出力を空にして標準エラーに診断1行を出す

```
