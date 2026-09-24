# 終了猶予の設定形式

終了猶予の設定位置を定義する草案。終了要求の具体的な機構と期限の起点は未決。

## Requirements

### REQ-095: process内の終了猶予設定

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A100,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A68
- verification: unit

終了猶予設定はTOMLの "process" 内の "shutdown-grace-seconds" に置く。既定は5秒、指定範囲は1〜300秒の整数とする。

### REQ-096: 終了猶予をfilteredだけに適用する

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A101
- verification: unit

新しい終了猶予は今回は "filtered" だけに適用し、"host"・"none" の終了方式は既存どおりとする。"host"・"none" に明示的な終了猶予設定が残っていれば、使われない旨を警告する。

### REQ-097: 終了猶予設定の合成と検査

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A102
- verification: unit

"process.shutdown-grace-seconds" は上位の明示値で上書きし、省略なら下の段の値を使い、全段省略なら5秒とする。"host"・"none" でも整数かつ1〜300秒の範囲を検査する。猶予設定がある場合は "network.mode" の明示を必須とし、別の段からの引き継ぎも認める。

## Examples

```gherkin
@id=EX-204 @about=REQ-095 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A100
Scenario: processから終了猶予を読む
  Given process.shutdown-grace-secondsに10を指定している
  When 設定を解釈する
  Then 終了猶予の指定値を10秒として扱う

@id=EX-205 @about=REQ-095 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A100
Scenario: 終了猶予の省略時は5秒を使う
  Given 終了猶予設定を省略している
  When 終了猶予の設定値を決定する
  Then 既定値の5秒を使う

@id=EX-206 @about=REQ-095 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A100,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A68
Scenario: 整数秒以外の終了猶予を拒否する
  Given process.shutdown-grace-secondsに1.5を指定している
  When 値を検査する
  Then 入力エラーにする

@id=EX-207 @about=REQ-096 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A101
Scenario: filteredでは終了猶予を適用する
  Given modeがfilteredで終了猶予を10秒に指定している
  When 主コマンド終了後に子プロセスを終了させる
  Then 10秒の終了猶予を適用する

@id=EX-208 @about=REQ-096 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A101
Scenario: hostでは残る猶予設定を警告して使わない
  Given modeにhostを明示し終了猶予を10秒に指定している
  When 起動する
  Then 猶予設定が使われない旨を警告する
  And hostの既存の終了方式を維持する

@id=EX-209 @about=REQ-096 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A101
Scenario: noneでも終了方式を変更しない
  Given modeにnoneを明示し終了猶予を10秒に指定している
  When 起動する
  Then 猶予設定が使われない旨を警告する
  And noneの既存の終了方式を維持する

@id=EX-210 @about=REQ-097 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A102
Scenario: 終了猶予は上位の明示値で上書きする
  Given 下の段の終了猶予は5秒で上の段は10秒である
  When 設定を合成する
  Then 終了猶予は10秒になる

@id=EX-211 @about=REQ-097 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A102
Scenario: 上位で省略すれば下位の終了猶予を使う
  Given 下の段の終了猶予は10秒で上の段には指定がない
  When 設定を合成する
  Then 終了猶予は10秒になる

@id=EX-212 @about=REQ-097 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A102
Scenario: hostでも終了猶予の範囲外指定を拒否する
  Given modeにhostを明示し終了猶予に301を指定している
  When 起動前に値を検査する
  Then 終了猶予の範囲外指定を入力エラーにする

@id=EX-213 @about=REQ-097 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A102
Scenario: 終了猶予を指定したらmodeの明示を求める
  Given どの段にもmodeが指定されていない
  And process.shutdown-grace-secondsに5を指定している
  When 起動前に設定を検査する
  Then modeの未指定を設定エラーにする
```
