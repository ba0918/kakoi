# 国際化ドメイン名の入力

日本語等を含むDNS名の変換と、変換後の名前の利用を扱う草案。検査オプション、依存の版、計画表示の項目は未決。

## Requirements

### REQ-011: 国際化ドメイン名の変換

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A12, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A13
- verification: unit

日本語等を含むDNS名を入力できる。既存ライブラリによるUTS #46の非移行処理でASCII名へ変換する。変換・検証に失敗した名前は入力エラーにする。

### REQ-012: 変換済みの名前の共有

- kind: invariant
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A12
- verification: unit

許可判定と名前解決は同じ変換済みASCII名を使う。変換後にもIP・ポート制限と内部IPの検査を適用する。

### REQ-013: 変換結果の確認

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A12
- verification: unit

計画表示で、実際に使う変換済みASCII名を確認できる。

## Examples

```gherkin
@id=EX-021 @about=REQ-011 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A13
Scenario: 全角英字を変換する
  Given DNS名の入力が "ＥＸＡＭＰＬＥ.com" である
  When ASCII名へ変換する
  Then "example.com" になる

@id=EX-022 @about=REQ-011 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A13
Scenario: 変換できない名前を拒否する
  Given DNS名の入力が変換・検証に失敗する名前である
  When ASCII名へ変換する
  Then 入力エラーにする
```
