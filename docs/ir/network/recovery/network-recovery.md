# 通信制限の復帰再試行

安全な遮断を維持できている間の再試行を定義する草案。各試行の時間上限・設定可否・再発時の待ち時間リセット・通知形式と理由の同一性判定は未決。

## Requirements

### REQ-066: 復帰失敗後の待ち時間と継続

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A71
- verification: unit

安全な遮断を維持できている場合、通信制限の再構築に失敗したら1・2・4・8・16秒、以降30秒待って再試行する。試行を重ねず、隔離環境が動く間は回数制限なく再試行する。再試行中も安全な遮断を維持する。

### REQ-067: 復帰再試行の状態変化を通知する

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A72
- verification: unit

復帰再試行中は遮断開始・失敗理由の変化・復帰成功を通知する。同じ理由で失敗し続けている間は同じ通知を繰り返さない。

## Examples

```gherkin
@id=EX-123 @about=REQ-066 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A71
Scenario: 再構築失敗後に待ってから次を試す
  Given 障害で安全に通信を遮断している
  When 最初の再構築に失敗する
  Then 1秒待ってから次の再構築を試す
  And 遮断を維持する

@id=EX-124 @about=REQ-066 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A71
Scenario: 失敗が続いても回数で復帰を諦めない
  Given 安全な遮断を維持できていて隔離環境が動いている
  And 再構築に失敗し1・2・4・8・16秒の待ち時間を順に使って再試行した
  When 再構築がさらに失敗する
  Then 30秒待って次を試す
  And その後も失敗ごとに30秒待って再試行を続ける

@id=EX-125 @about=REQ-066 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A71
Scenario: 再構築の試行を同時に重ねない
  Given 通信制限を再構築する試行が実行中である
  When その試行がまだ完了していない
  Then 次の再構築の試行を開始しない

@id=EX-126 @about=REQ-067 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A72
Scenario: 同じ理由の再試行失敗を繰り返し通知しない
  Given 遮断開始と失敗理由を通知済みである
  When 再試行が同じ理由で失敗し続ける
  Then 同じ失敗通知を繰り返さない

@id=EX-127 @about=REQ-067 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A72
Scenario: 失敗理由の変化を知らせる
  Given 遮断中に再構築の失敗理由を通知済みである
  When 次の再試行で失敗理由が変わる
  Then 変わった失敗理由を通知する

@id=EX-128 @about=REQ-067 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A72
Scenario: 復帰に成功したことを知らせる
  Given 障害で遮断を開始したことを通知済みである
  When 通信制限を再構築し確認して通信が復帰する
  Then 復帰成功を通知する
```
