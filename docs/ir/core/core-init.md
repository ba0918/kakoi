# 初期プロファイルの書き出し

既存仕様から継承した本体の検査用表現。同じ責務を一文書にまとめ、見出しと要求IDで参照する。正本は責務別spec文書群。

## 要求

### REQ-255: helpとversion
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

--helpと--versionは標準出力へ使い方または版を出して0で終わり、カレントディレクトリの取得を必要としない。他の引数との併用はusage。

### REQ-256: initの作成内容
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

initは設定ディレクトリのprofile/NAME.tomlへ同梱examples/profile/default.tomlと同一バイト列を書き、欠けている設定ディレクトリの祖先、自身、profile/、secrets/を作る。

### REQ-257: initのリンクとモード
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

initは書き出す名前より上のパス成分のリンクを辿り、リンク切れか非ディレクトリならpath。secrets/は新規・既存とも0700にし、profile/と出力ファイルはumaskに従う。

### REQ-258: initの出力と上書き拒否
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

init成功時は実体化しない構築済み出力パス1行だけを標準出力へ出し0で終わる。出力名に何か既存ならリンク切れを含め名前自身を辿らず、何も書かずpathで125。forceは持たない。書き込み失敗の説明は対象パスを含む。

### REQ-259: initの独立した検査
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

initは文法、HOMEの検査、書き込みの順に実行する。XDG_CONFIG_HOMEがあってもHOME検査を行う。ポリシー読込、workspace導出、マウント解決、bwrap所在確認、入れ子検出を行わない。NAME以外の引数はusage。

## 具体例

```gherkin
@id=EX-495 @about=REQ-255 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: helpとversion
  Given 本体の既存仕様を適用する
  When 削除済みカレントディレクトリから--helpを使う
  Then 使い方を標準出力へ出して0で終わる

@id=EX-496 @about=REQ-256 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: initの作成内容
  Given 本体の既存仕様を適用する
  When 設定ディレクトリが存在しない状態でinitする
  Then profile/default.tomlとsecrets/と欠けていた祖先を作る

@id=EX-497 @about=REQ-257 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: initのリンクとモード
  Given 本体の既存仕様を適用する
  When 既存secrets/が0755の状態でinitする
  Then secrets/を0700にする

@id=EX-498 @about=REQ-258 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: initの出力と上書き拒否
  Given 本体の既存仕様を適用する
  When default.tomlにリンク切れがある状態でinitする
  Then リンク先を作らずpathで125となる

@id=EX-499 @about=REQ-259 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: initの独立した検査
  Given 本体の既存仕様を適用する
  When KAKOI=1かつbwrapがない環境でinitする
  Then 書き込み条件を満たせば入れ子警告なしで成功する

```
