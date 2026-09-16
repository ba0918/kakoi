# 名前解決ごとの上流問い合わせ回数

kakoi-net自身が送る問い合わせの回数上限を定義する草案。名前解決1件の境界と、その他の資源上限は未決。

## 要求

### REQ-121: 上流問い合わせ回数の上限を設定できる

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A124
- 検証: unit

名前解決1件あたりの上流問い合わせ回数は既定64回とし、"network.dns-max-upstream-queries" で1〜4096の整数に変更できる。0・無制限・小数は認めない。上位指定優先・省略時継承とし、どの段にも指定がなければ64回とする。

### REQ-122: 再試行も含め上流への送信を数える

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A124
- 検証: unit

kakoi-net自身が上流へ送るDNS問い合わせを1送信ごとに数える。別候補への切替、CNAME参照、再送、通信方式を変えての再問い合わせ、設定変更によるやり直しも含め、回数をリセットしない。上流DNS内部の処理と通信層のパケット再送は数えない。上限ちょうどまで送信でき、さらに送信が必要なら解決失敗とする。時間と段数の上限も独立して適用する。

## 具体例

```gherkin
@id=EX-267 @about=REQ-121 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A124
Scenario: 回数上限の上下限を受け付ける
  Given dns-max-upstream-queriesに整数1または4096を指定している
  When 設定を検査する
  Then 有効な回数上限として受け付ける

@id=EX-268 @about=REQ-121 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A124
Scenario: 範囲外や整数でない回数上限を拒否する
  Given dns-max-upstream-queriesに0または4097または小数または無制限を指定している
  When 設定を検査する
  Then 設定エラーにする

@id=EX-269 @about=REQ-121 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A124
Scenario: 上位で省略したら下位の上限を使う
  Given 下位で128回を指定し上位では省略している
  When 設定を合成する
  Then 回数上限は128になる

@id=EX-270 @about=REQ-122 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A124
Scenario: 設定変更によるやり直しでも残り回数を引き継ぐ
  Given 回数上限64の解決処理で63回の上流問い合わせを送った
  When ホストDNS設定の変更により問い合わせ直す
  Then あと1回だけ上流問い合わせを送信できる
  And さらに送信が必要になれば解決失敗とする

@id=EX-271 @about=REQ-122 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A124
Scenario: 通信層の再送をDNS問い合わせとして重複計上しない
  Given kakoi-netがDNS問い合わせを1回送った
  When 通信層がそのパケットを再送する
  Then DNS問い合わせの送信回数は1回のままとする

@id=EX-272 @about=REQ-122 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A124
Scenario: 回数が残っていても時間上限を超えて続行しない
  Given 上流問い合わせ回数に余裕がある
  When 名前解決全体の時間上限に達する
  Then 回数の余裕を理由に解決処理を続行しない
```
