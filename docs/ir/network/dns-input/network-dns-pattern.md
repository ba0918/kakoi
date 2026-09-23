# DNS名の指定範囲

DNS名の完全一致、子孫名を指定するワイルドカード、比較時の表記の扱いを定義する草案。国際化名の入力と名前解決の手順は含まない。

## Requirements

### REQ-003: DNS名の完全一致と子孫指定

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A6
- verification: unit

ワイルドカードのないDNS名の指定は、その名前だけに一致する。先頭の "*." を付けた指定は、深さを問わず子孫の名前に一致し、親自身には一致しない。

### REQ-004: ワイルドカードの位置制限

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A6, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A14
- verification: unit

DNS名の指定にワイルドカードを使う場合、入力時点で先頭の半角 "*." だけを認め、名前の部分を変換する。それ以外の位置や形で使った場合は入力エラーにする。全角の星は半角に変換して受け付けない。

### REQ-010: DNS名の比較と補完の禁止

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A11
- verification: unit

DNS名の比較では英字の大小文字を区別せず、末尾のドット1個は取り除く。末尾ドットの有無によらず、指定は完成した名前として扱い、ホストの検索用ドメインを補わない。

## Examples

```gherkin
@id=EX-004 @about=REQ-003 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A6
Scenario: 複数段の子孫名に一致する
  Given DNS名の指定が "*.example.com" である
  When "v1.api.example.com" と照合する
  Then 一致する

@id=EX-005 @about=REQ-003 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A6
Scenario: 子孫指定は親自身に一致しない
  Given DNS名の指定が "*.example.com" である
  When "example.com" と照合する
  Then 一致しない

@id=EX-006 @about=REQ-004 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A6
Scenario: 先頭の星とドットを受け付ける
  Given DNS名の指定が "*.example.com" である
  When ワイルドカードの形を検査する
  Then 受け付ける

@id=EX-007 @about=REQ-004 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A6
Scenario: 名前の途中の星を拒否する
  Given DNS名の指定が "api*.example.com" である
  When ワイルドカードの形を検査する
  Then 入力エラーにする

@id=EX-017 @about=REQ-010 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A11
Scenario: 大小文字の違いを区別しない
  Given DNS名の指定が "API.Example.com" である
  When "api.example.com" と照合する
  Then 一致する

@id=EX-018 @about=REQ-010 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A11
Scenario: 末尾ドットの有無を区別しない
  Given DNS名の指定が "api.example.com." である
  When "api.example.com" と照合する
  Then 一致する

@id=EX-019 @about=REQ-010 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A11
Scenario: 名前を補完して許可しない
  Given DNS名の指定が "api" である
  When "api.example.com" と照合する
  Then 一致しない

@id=EX-020 @about=REQ-004 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A14
Scenario: 全角の星を拒否する
  Given DNS名の指定が "＊.example.com" である
  When ワイルドカードの形を検査する
  Then 入力エラーにする
```
