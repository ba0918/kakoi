# ポート指定

通信許可に書くポートの記法、有効範囲、重複と入力エラーの扱いを定義する草案。

## 要求

### REQ-005: ポートの明示指定

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A7, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A8
- 検証: unit

ポートは "ports" の文字列配列で指定する。単一番号は "443"、範囲は "8000-8010" の形とし、番号と範囲を同じ配列に書ける。全ポートは "*" だけを要素とする配列で指定する。ポート指定を省略した場合は入力エラーにする。

### REQ-006: 空と全ポート混在の拒否

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A8
- 検証: unit

ポートの配列が空の場合、または "*" に他の指定を混ぜた場合は入力エラーにする。

### REQ-007: ポート番号と範囲の境界

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A9
- 検証: unit

ポート番号は1〜65535とし、"*" はこの範囲を表す。範囲指定は両端を含み、両端が同じ場合は単一番号と同じとする。開始番号が終了番号より大きい場合は入力エラーにする。

### REQ-008: 重複と重なりの許容

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A10
- 検証: unit

番号の重複と範囲の重なりは受け付け、許可範囲は指定の和集合とする。

### REQ-009: ポートの表記と型の制限

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A10
- 検証: unit

番号に先頭ゼロ・空白・プラス符号がある場合、サービス名で指定した場合、文字列でない数値を指定した場合は入力エラーにする。

## 具体例

```gherkin
@id=EX-008 @about=REQ-005 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A8
Scenario: 番号と範囲を併記する
  Given ポート指定が ["443", "8443", "8000-8010"] である
  When ポート指定の記法を検査する
  Then 受け付ける

@id=EX-009 @about=REQ-005 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A7
Scenario: 省略を拒否する
  Given ポート指定が省略されている
  When ポート指定を検査する
  Then 入力エラーにする

@id=EX-010 @about=REQ-006 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A8
Scenario: 全ポートだけの指定を受け付ける
  Given ポート指定が ["*"] である
  When ポート指定を検査する
  Then 受け付ける

@id=EX-011 @about=REQ-006 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A8
Scenario: 全ポートとの混在を拒否する
  Given ポート指定が ["*", "443"] である
  When ポート指定を検査する
  Then 入力エラーにする

@id=EX-012 @about=REQ-006 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A8
Scenario: 空配列を拒否する
  Given ポート指定が空配列である
  When ポート指定を検査する
  Then 入力エラーにする

@id=EX-013 @about=REQ-007 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A9
Scenario: 両端が同じ範囲を受け付ける
  Given ポート指定が ["443-443"] である
  When ポート指定を解釈する
  Then 443番だけを許可する

@id=EX-014 @about=REQ-007 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A9
Scenario: 逆転した範囲を拒否する
  Given ポート指定が ["8010-8000"] である
  When ポート指定を検査する
  Then 入力エラーにする

@id=EX-015 @about=REQ-008 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A10
Scenario: 重なる範囲を受け付ける
  Given ポート指定が ["8000-8010", "8005-8020"] である
  When ポート指定を解釈する
  Then 8000〜8020番を許可する

@id=EX-016 @about=REQ-009 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A10
Scenario: サービス名を拒否する
  Given ポート指定が ["https"] である
  When ポート指定を検査する
  Then 入力エラーにする
```
