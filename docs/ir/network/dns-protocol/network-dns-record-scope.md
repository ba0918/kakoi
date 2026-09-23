# DNSレコード照会の対応範囲

初版の対応方針を定義する草案。対応種別の完全な一覧と未対応・入力不正の応答は未決。

## Requirements

### REQ-130: アドレス以外の照会も扱い通信許可の根拠を広げない

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A130
- verification: unit

初版はA/AAAAに加えTXT・MX・SRV・HTTPSのレコード照会にも対応する。DNS更新・ゾーン転送・全種類の一括照会は対象外とする。レコード内の情報から通信許可を自動で広げず、CNAME以外で紹介された参照先名には別途許可を要する。

## Examples

```gherkin
@id=EX-295 @about=REQ-130 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A130
Scenario: 許可名のTXT照会を扱う
  Given 問い合わせ名は許可済みである
  When TXTレコードを問い合わせる
  Then アドレス以外という理由だけでは拒否しない

@id=EX-296 @about=REQ-130 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A130
Scenario: レコード内のIPから許可を自動追加しない
  Given 許可名のHTTPSレコードにIPのヒントが含まれている
  When その応答を受け取る
  Then そのヒントだけを根拠に通信許可を追加しない

```
