# 終了猶予の共通期限

filteredの終了猶予を定義する草案。猶予前の遮断処理の時間上限と停止機構の実証は未決。

## 要求

### REQ-105: 最初の終了要求から全体で1本の期限を使う

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A110
- 検証: unit

"filtered" の終了処理で通信・公開を止めて最初の終了要求を送ったら、その時点から設定済みの猶予時間を計り、主コマンドと子プロセス全体で1本の期限を使う。主コマンドの途中の終了や新しい子プロセスの出現で期限を延長しない。

## 具体例

```gherkin
@id=EX-227 @about=REQ-105 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A110
Scenario: 主コマンドが途中で終了しても子の猶予を数え直さない
  Given 通信と公開を止め最初の終了要求から5秒の猶予を開始した
  When 2秒後に主コマンドが終了する
  Then 残る子プロセスの猶予は3秒である

@id=EX-228 @about=REQ-105 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A110
Scenario: 新しい子プロセスが出現しても期限を延長しない
  Given 終了猶予の期限まで1秒残っている
  When 新しい子プロセスが出現する
  Then 新しい子プロセスも既存の期限の対象とする
  And 猶予期限を延長しない

@id=EX-229 @about=REQ-105 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A110
Scenario: 主コマンドが先に終了した場合も終了要求から計る
  Given 主コマンドが終了し子プロセスが残っている
  And 終了猶予は5秒である
  When 通信と公開を止め子プロセスへ最初の終了要求を送る
  Then その時点から5秒後を共通の猶予期限とする
```
