# 端末の保護

既存仕様から継承した本体の検査用表現。同じ責務を一文書にまとめ、見出しと要求IDで参照する。この IR が正本である。

## Requirements

### REQ-280: アーキテクチャの検査
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

bwrapの--seccompで渡すフィルタは最初にアーキテクチャを確認し、x86_64以外のシステムコールで全スレッドを含むプロセス全体を終了させる。

### REQ-281: x32の拒否
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

システムコール番号に0x40000000が立つx32呼び出しはプロセス全体を終了させる。32ビットとx32バイナリを隔離内で動かさない。

### REQ-282: TIOCSTIの拒否
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

ioctlの第2引数の下位32ビットがTIOCSTIならEPERMとする。上位ビットが立っていても拒否する。

### REQ-283: 端末保護の範囲
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

上記以外のシステムコールを許す。new-sessionは使わず制御端末を維持する。TIOCLINUX、端末応答を使う注入、表示サーバ経由の入力合成は対象外としてREADMEに記し、最後の経路は同梱プロファイルのhideとunsetで消す。

## Examples

```gherkin
@id=EX-516 @about=REQ-280 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: アーキテクチャの検査
  Given 本体の既存仕様を適用する
  When x86_64以外のアーキテクチャをフィルタへ渡す
  Then 最初の判定でプロセス全体を終了させる

@id=EX-517 @about=REQ-281 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: x32の拒否
  Given 本体の既存仕様を適用する
  When x32ビット付きシステムコールを呼ぶ
  Then SIGSYSによる終了となる

@id=EX-518 @about=REQ-282 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: TIOCSTIの拒否
  Given 本体の既存仕様を適用する
  When TIOCSTIの上位ビットを立ててioctlする
  Then EPERMとなる

@id=EX-519 @about=REQ-283 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 端末保護の範囲
  Given 本体の既存仕様を適用する
  When TIOCSTI以外の端末サイズ取得ioctlを呼ぶ
  Then フィルタは呼び出しを許す

```
