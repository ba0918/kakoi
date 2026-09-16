# 許可の正本とDNS入力

起動時のポリシーと、実行中に得るDNS情報の役割を区別する草案。上流応答と問い合わせの具体的な照合方法、制御機構の保護方法は未決。

## 要求

### REQ-028: DNS由来の許可を作る情報源

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A33
- 検証: unit

DNS由来のIP許可を作る情報源は、管理するDNS経路で送った問い合わせとの対応を確認できた上流応答に限る。アプリが別経路で取得した結果や申告したIPからは許可を追加しない。そのIPへの通信は既存の有効なIP・ポート許可で判定する。

### REQ-029: 起動時のポリシーを許可の正本とする

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A34
- 検証: unit

許可の正本は起動時に確定したポリシーとする。アプリの申告を許可の根拠として信用せず、ポリシーを変更・拡張させない。DNS応答は許可済みの名前の現在のIPを求める入力として扱い、許可ルール自体を追加する根拠にはしない。

## 具体例

```gherkin
@id=EX-047 @about=REQ-028 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A33
Scenario: アプリのIP申告で許可を追加しない
  Given アプリが別経路で取得した名前とIPの対応を申告する
  When DNS由来のIP許可を更新する
  Then その申告を根拠とした許可を追加しない

@id=EX-048 @about=REQ-028 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A33
Scenario: 別経路で取得したIPでも既存の許可があれば通す
  Given アプリが別経路で取得したIPとポートに有効な許可がある
  When そのIPとポートに許可されたトランスポートで通信する
  Then 既存の許可に従って通信を通す

@id=EX-049 @about=REQ-028 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A33
Scenario: 別経路で取得しただけのIPへの通信を拒否する
  Given アプリが別経路で取得したIPとポートに有効な許可がない
  When そのIPとポートに新規通信を試みる
  Then 通信を拒否する

@id=EX-050 @about=REQ-029 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A34
Scenario: アプリが許可対象の追加を申告してもポリシーを拡張しない
  Given 起動時にポリシーが確定している
  When アプリが新しい宛先やポートを許可対象として申告する
  Then その申告によってポリシーを変更しない
```
