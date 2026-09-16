# ホストOSのCAを起動時に採用

今回合意した追加・改訂部分の草案。方式の実証は別途必要。

## 要求

### REQ-147: ホストOSのCAを起動時に採用

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A145
- 検証: unit

TLSは起動時にホストOSのCA証明書を読み込む。独自CAはOS側へ登録する。初版は専用CAファイル指定と証明書検証無効化を設けない。起動後のCA変更は再起動で反映する。

## 具体例

```gherkin
@id=EX-326 @about=REQ-147 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A145
Scenario: 起動時にホストの信頼元を取得する
  Given 明示TLS上流を使う
  When 環境を起動する
  Then ホストOSのCA証明書を読み込む

@id=EX-327 @about=REQ-147 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A145
Scenario: 起動後の変更を自動採用しない
  Given 環境が起動済みである
  When ホストOSのCA証明書が変更される
  Then 変更の反映には環境の再起動を要する

```
