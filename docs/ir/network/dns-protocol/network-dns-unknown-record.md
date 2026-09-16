# 新しいDNSレコード種別の中継

通常の読み取り照会で扱うレコード種別の範囲を定義する草案。

## 要求

### REQ-149: 未知の通常レコードを中継する

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A152
- 検証: unit

許可名への通常の読み取り照会は固定したレコード種別一覧に限定せず、RFC 3597に従い新しい種別のデータも内容を解釈せず中継する。不正形式の検査は行う。通信許可を追加する根拠は管理経路で検査したA/AAAAだけとする。DNS更新・ゾーン転送・ANYは対象外を維持する。

## 具体例

```gherkin
@id=EX-330 @about=REQ-149 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A152
Scenario: 新しい通常レコードを照会できる
  Given 許可名への通常の読み取り照会である
  And 新しいレコード種別はRFC 3597の未知RRに該当する
  When 不正形式の検査を通ったデータを中継する
  Then データの内容を解釈せず中継する

@id=EX-331 @about=REQ-149 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A152
Scenario: 未知のデータ中のIPらしい値を許可根拠にしない
  Given 中継する新しい種別のデータにIPらしい値がある
  When そのデータを中継する
  Then その値を根拠とした通信許可を追加しない
```
