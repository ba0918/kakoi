# 空の通信許可と公開設定

明示的に選んだfilteredで許可・公開設定が空の場合を定義する草案。モード未指定で空配列や補助設定だけがある場合は未決。

## 要求

### REQ-085: 空のfiltered設定での起動

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A90
- 検証: unit

"filtered" で通信許可・公開設定がどちらも空または未指定でも、有効な設定として起動する。アプリの外部通信・ホスト接続・待受公開は許可せず、隔離環境内ループバックは使える。

## 具体例

```gherkin
@id=EX-175 @about=REQ-085 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A90
Scenario: filteredだけを指定して起動する
  Given network.modeにfilteredを指定し通信許可と公開設定を省略している
  When 通信制限の準備に成功して起動する
  Then 空の許可設定を理由に起動エラーにしない
  And アプリの外部通信とホスト接続と待受公開を許可しない

@id=EX-176 @about=REQ-085 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A90
Scenario: 空の許可でも隔離環境内ループバックは使える
  Given filteredで通信許可と公開設定がどちらも空である
  When アプリが隔離環境内ループバックで別の処理と通信する
  Then そのループバック通信を許可する
```
