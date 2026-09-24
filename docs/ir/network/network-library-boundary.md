# ネットワークライブラリの責務

coreとnetの分担と、CLIを通さない利用の契約を定義する草案。

## Requirements

### REQ-150: 計画と実行の責務を分ける

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A153, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4, docs/decision/brainstorm/2026-09-25-library-boundary.md#A1
- verification: review
- how_to_verify: crates/kakoi-coreを読み、設定の検査・合成・計画を担当し、引数の解釈、プロセスの環境変数とカレントディレクトリの読み取り、標準出力と標準エラーへの書き込み、コマンドの実行、通信の起動・監督・停止をしないことを確かめる。crates/kakoi-netが通信の起動・監督・停止を担い、CLIを通さずRustプログラムから使える公開の入口を持つこと、DNS応答の採否と許可期限の判断がOS操作と分かれて単独で検証できる構造であることを確かめる。

kakoi-coreは設定の検査・合成・計画を担当する。kakoi-coreは引数を解釈せず、プロセスの環境変数とカレントディレクトリを読まず、標準出力と標準エラーに書かず、コマンドを実行せず、通信を起動・監督・停止しない。パスの事実とマウント一覧とrw-copyの複製元を読むこと、bwrapに渡す記述子を用意すること、bwrapのコマンドを組み立てることはkakoi-coreが行う。kakoi-netが通信の起動・監督・停止を担い、CLI以外のRustプログラムからも使える形にする。DNS応答の採否や許可期限の判断はOS操作と分けて検証できる構造にする。

## Examples

```gherkin
@id=EX-332 @about=REQ-150 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A153
Scenario: CLI以外から通信の実行を利用できる
  Given CLI以外のRustプログラムがnetを利用する
  When 通信の起動と監督と停止を行う
  Then CLIを経由せず利用できる

@id=EX-333 @about=REQ-150 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A153
Scenario: 判断をOS操作へ混ぜない
  Given DNS応答の採否や許可期限の判断を検証する
  When 判断部分の責務を確認する
  Then OS操作と分けて検証できる
```
