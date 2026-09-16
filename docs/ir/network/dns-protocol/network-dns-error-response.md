# DNS拒否と解決失敗の応答

kakoi-netが返すDNS失敗応答を定義する草案。詳細理由の通知形式とその他の入力不正の応答は未決。

## 要求

### REQ-123: ポリシー拒否と解決失敗の応答を分ける

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A125
- 検証: unit

未許可の名前や解決先IPの全件禁止というポリシー拒否はREFUSEDを返す。上流で解決できない場合、時間・段数・回数の上限超過、CNAME循環、変更後のホストDNS設定取得失敗はSERVFAILを返す。採用可能な上流のNXDOMAIN/NODATAは既存方針どおり維持する。

## 具体例

```gherkin
@id=EX-273 @about=REQ-123 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A125
Scenario: 問い合わせ対象でない名前を拒否する
  Given 問い合わせ名は未許可で許可名のCNAME参照先でもない
  When 管理するDNS経路で問い合わせを受ける
  Then REFUSEDを返す

@id=EX-274 @about=REQ-123 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A125
Scenario: 解決先の全IPが禁止されていたら拒否する
  Given 許可名のDNS応答に含まれる解決先IPがすべて禁止されている
  When その解決結果を検査する
  Then REFUSEDを返す

@id=EX-275 @about=REQ-123 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A125
Scenario: 解決処理の上限に達して失敗したら処理失敗を返す
  Given 名前解決に必要な処理がまだ残っている
  When 時間または段数または問い合わせ回数の制限で解決失敗になる
  Then SERVFAILを返す

@id=EX-276 @about=REQ-123 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A125
Scenario: 全上流候補で解決できなければ処理失敗を返す
  Given 明示した上流の全候補が応答しないか処理失敗または拒否を返した
  When 名前解決を失敗として終える
  Then SERVFAILを返す

@id=EX-277 @about=REQ-123 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A125
Scenario: 上流の有効な不存在応答を処理失敗に置き換えない
  Given 上流から採用可能なNXDOMAINまたはNODATA応答を得た
  When 名前解決の結果を返す
  Then その不存在応答を維持する
```
