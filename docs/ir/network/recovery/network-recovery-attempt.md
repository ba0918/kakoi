# 復帰試行の上限と成功時のリセット

復帰再試行の設定方針を定義する草案。設定キーと打切り・回収機構の実証は未了。

## 要求

### REQ-143: 復帰試行の上限と成功時のリセット

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A141
- 検証: unit

復帰の各試行は既定10秒で打ち切り、設定で1〜300秒に変更できる。既存の1・2・4・8・16秒、以降30秒の待機列を維持する。復帰成功で失敗回数をリセットし、再発時は最初の待機列から始める。試行を重ねず安全な遮断を維持する。

## 具体例

```gherkin
@id=EX-318 @about=REQ-143 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A141
Scenario: 復帰試行が期限内に完了しなければ打ち切る
  Given 復帰試行の時間上限を省略している
  When 試行が未完了のまま10秒に達する
  Then その試行を打ち切る
  And 安全な遮断を維持する

@id=EX-319 @about=REQ-143 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A141
Scenario: 成功後に再発したら待機列をリセットする
  Given 再試行の待ち時間が30秒まで増えた後に復帰に成功した
  When 故障が再発しその最初の再構築に失敗する
  Then 次の試行までの待ち時間を1秒とする

```
