# 実行中のネットワーク通知

実行中の通知先を定義する草案。詳細な表示形式、出力待ち・保持量の上限、出力先回復後の扱いは未決。

## Requirements

### REQ-068: 標準エラーへのネットワーク通知

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A73
- verification: unit

実行中のネットワーク通知は標準エラーへ1件1行で出し、kakoiの通知と分かる接頭辞を付ける。標準出力には通知を混ぜない。アプリの標準エラーと同じ出力先を使い、専用の受信設定を要求しない。

### REQ-069: 通知不能で通信制御を止めない

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A74
- verification: unit

ネットワーク通知先が閉じている・読み手が滞っている場合は通知の欠落を許容する。通知出力待ちで通信制限・遮断・復帰・終了処理を止めず、制限を維持できる限りアプリを継続する。

## Examples

```gherkin
@id=EX-129 @about=REQ-068 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A73
Scenario: 追加設定なしで標準エラーに通知する
  Given 標準エラーへ書き込める状態でアプリを実行している
  And 専用の通知受信先を設定していない
  When ネットワークの状態変化を通知する
  Then kakoiの通知と分かる接頭辞付きの1行を標準エラーへ出す

@id=EX-130 @about=REQ-068 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A73
Scenario: パイプで渡す標準出力に通知を混ぜない
  Given アプリの標準出力を別のコマンドへ渡している
  When ネットワークの状態変化を通知する
  Then その通知を標準出力には出さない

@id=EX-131 @about=REQ-069 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A74
Scenario: 通知先が閉じてもアプリを終了させない
  Given 通信制限を維持できている
  When ネットワーク通知先が閉じていて通知を書き込めない
  Then 通知の欠落を許容してアプリを継続する

@id=EX-132 @about=REQ-069 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A74
Scenario: 通知の読み手が止まっても遮断を待たせない
  Given 通知先の読み手が止まり出力が詰まっている
  When 通信を遮断する必要が生じる
  Then 通知の出力完了を待たずに遮断する

@id=EX-133 @about=REQ-069 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A74
Scenario: 通知待ちで環境の終了を止めない
  Given 通知先の読み手が止まり出力が詰まっている
  When 隔離環境を終了する
  Then 通知の出力完了を待たずに終了処理を進める
```
