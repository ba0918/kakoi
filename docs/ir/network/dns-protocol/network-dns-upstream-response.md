# 明示上流の応答と候補切替

明示した上流DNSの応答を受けた際の扱いを定義する草案。その他のエラー・全候補失敗時の応答・失敗キャッシュ・通信失敗の詳細と待ち時間は未決。

## Requirements

### REQ-114: 不存在応答を理由に次の上流を試さない

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A119
- verification: unit

明示上流から採用可能なNXDOMAINまたはNODATA応答を得たら、その問い合わせの結果として採用し、それだけを理由に次の候補へ問い合わせない。応答の正当性確認を省略しない。

### REQ-115: 上流の処理失敗と拒否では次の指定済み候補を試す

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A120
- verification: unit

明示上流からSERVFAILまたはREFUSEDを受けた場合は次の指定済み候補を試し、問い合わせ全体の時間・作業量の上限を引き継ぐ。先の上流が独自の方針で拒否しても次の上流が回答すれば採用する。kakoi自身の通信許可や上流に求める信頼条件は緩めない。

## Examples

```gherkin
@id=EX-248 @about=REQ-114 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A119
Scenario: 名前が存在しないという回答で終了する
  Given 明示した上流DNSに次の候補がある
  When 現在の候補から採用可能なNXDOMAIN応答を得る
  Then その不存在応答を結果として採用する
  And 不存在を理由に次の候補へ問い合わせない

@id=EX-249 @about=REQ-114 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A119
Scenario: 要求した種類のデータがないという回答で終了する
  Given 明示した上流DNSに次の候補がある
  When 現在の候補から採用可能なNODATA応答を得る
  Then その不存在応答を結果として採用する
  And 不存在を理由に次の候補へ問い合わせない

@id=EX-250 @about=REQ-115 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A120
Scenario: 処理失敗を受けたら次の指定済み候補を試す
  Given 明示した上流DNSに次の候補があり全体上限にも余裕がある
  When 現在の候補からSERVFAILを受ける
  Then 次の指定済み候補を試す
  And 問い合わせ全体の時間と作業量の上限をリセットしない

@id=EX-251 @about=REQ-115 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A120
Scenario: 上流独自の拒否後も別候補の回答を採用できる
  Given 明示した候補1がREFUSEDを返した
  And 全体上限の範囲内で次の指定済み候補2を試している
  When 候補2から採用可能な応答を得る
  Then その応答を採用する
  And kakoi自身の通信許可と上流に求める信頼条件は維持する
```
