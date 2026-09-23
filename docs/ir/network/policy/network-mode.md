# 通信制限モードの選択

既存のhost・noneに加える通信制限モードを定義する草案。使わない設定の項目間整合検査、空の設定、既定モードとの整合は未決。

## Requirements

### REQ-082: filteredモードの明示的な選択

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A87
- verification: unit

既存の "network.mode" の "host"・"none" を残し、新しい "filtered" の明示で今回の通信制限を有効にする。許可・公開設定を書いただけではモードを自動変更しない。

### REQ-083: 明示モードと残る許可・公開設定

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A88
- verification: unit

採用されたモードが利用者の設定レイヤで明示された "host" または "none" なら、残る通信許可・公開設定を無視する旨を警告し、指定モードで起動する。別レイヤから継承したモードも明示に含む。どのレイヤにもモード指定がなく、空でない通信許可・公開設定がある場合は起動前エラーにする。"filtered" の準備失敗から "host" へは自動移行しない。

### REQ-084: 非使用設定の入力形式検査と環境検査の省略

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A89
- verification: unit

"host"・"none" で使わない通信許可・公開設定も入力形式を検査し、必須欄の欠落や不正なポート番号は起動前エラーにする。使わない設定のDNS問い合わせやインターフェースの存在確認は行わない。

## Examples

```gherkin
@id=EX-165 @about=REQ-082 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A87
Scenario: filteredを明示して通信制限を選ぶ
  Given network.modeにfilteredを指定し有効な通信許可を設定している
  When 起動のネットワークモードを決定する
  Then 今回の通信制限モードを選ぶ

@id=EX-166 @about=REQ-082 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A87
Scenario: 公開設定だけでモードを切り替えない
  Given network.modeがhostである
  When 公開設定が追加される
  Then 公開設定の存在だけでfilteredへ自動変更しない

@id=EX-167 @about=REQ-082 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A87
Scenario: 既存のnone指定を引き続き受け付ける
  Given 新しい許可と公開の設定を含まない既存のnetwork.mode="none"の設定がある
  When 起動のネットワークモードを決定する
  Then 既存のnoneモードとして扱う

@id=EX-168 @about=REQ-083 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A88
Scenario: 許可設定を残して明示的にhostへ切り替える
  Given 有効な通信許可設定が残っている
  And modeをfilteredからhostへ明示的に変更している
  When 起動する
  Then 通信許可設定を無視する旨を警告してhostで起動する

@id=EX-169 @about=REQ-083 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A88
Scenario: modeを指定せず許可設定だけ書いた場合は止める
  Given どの設定レイヤにもmodeを指定していない
  And 空でない通信許可設定がある
  When 起動前に設定を検査する
  Then 設定エラーにしてアプリを実行しない

@id=EX-170 @about=REQ-083 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A88
Scenario: 継承したnoneも明示モードとして扱う
  Given 下のレイヤでmodeにnoneを指定している
  And 上のレイヤに有効な公開設定がありmodeの上書きはない
  When 起動する
  Then 公開設定を無視する旨を警告してnoneで起動する

@id=EX-171 @about=REQ-084 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A89
Scenario: hostでも公開設定の必須欄欠落を拒否する
  Given modeにhostを明示している
  And 残る公開設定に必須のhost-portsがない
  When 起動前に設定を検査する
  Then 必須欄の欠落を設定エラーにする

@id=EX-172 @about=REQ-084 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A89
Scenario: noneでも不正なポート番号を拒否する
  Given modeにnoneを明示している
  And 残る通信許可のポートに65536を指定している
  When 起動前に設定を検査する
  Then 不正なポート番号を設定エラーにする

@id=EX-173 @about=REQ-084 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A89
Scenario: hostで使わないインターフェースの不在を検査しない
  Given modeにhostを明示し入力形式が有効な通信許可がある
  And その許可が指定するhost-interfaceは現在存在しない
  When 起動前に設定を検査する
  Then 使わない設定のインターフェース存在確認を行わない

@id=EX-174 @about=REQ-084 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A89
Scenario: noneで使わない許可名を問い合わせない
  Given modeにnoneを明示し入力形式が有効なDNS名の許可が残っている
  When 起動前に設定を検査する
  Then 使わない設定のDNS問い合わせを行わない
```
