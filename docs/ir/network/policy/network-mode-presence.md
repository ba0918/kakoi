# 新設定使用時のモード明示

新しいネットワーク設定の使用時にモードを明示する条件を定義する草案。

## Requirements

### REQ-087: 空配列と補助設定にもモード明示を要求する

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A92
- verification: unit

今回追加するネットワーク設定を使う場合は、空の許可・公開配列や補助設定だけでも "mode" の明示を必須とする。別レイヤから継承した明示モードも認める。新設定を一切使わない既存設定の扱いは変えない。

## Examples

```gherkin
@id=EX-179 @about=REQ-087 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A92
Scenario: 空の公開配列だけでもモード指定を求める
  Given どのレイヤにもmodeが指定されていない
  And network.publishに空配列を指定している
  When 起動前に設定を検査する
  Then modeの未指定を設定エラーにする

@id=EX-180 @about=REQ-087 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A92
Scenario: UDP待機時間だけでもモード指定を求める
  Given どのレイヤにもmodeが指定されていない
  And network.udp-idle-timeout-secondsだけを120に設定している
  When 起動前に設定を検査する
  Then modeの未指定を設定エラーにする

@id=EX-181 @about=REQ-087 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A92
Scenario: 下のレイヤのモード指定も認める
  Given 下のレイヤにmode="filtered"がある
  And 上のレイヤにはnetwork.publishの空配列だけを指定している
  When 起動前に設定を検査する
  Then modeの明示条件を満たすと判定する

@id=EX-182 @about=REQ-087 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A92
Scenario: 新設定を使わない既存設定の既定動作を変えない
  Given 既存の設定はmodeを省略し今回の新設定キーを使っていない
  When 起動前に設定を検査する
  Then 新たなmode未指定エラーにはせず既存の既定動作を維持する
```
