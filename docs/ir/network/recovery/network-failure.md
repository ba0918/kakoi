# 通信制限の準備失敗と維持不能

通信制限自体の失敗時の扱いを定義する草案。遮断を保証する機構・実証、通知形式と起動失敗時の終了コードは未決。

## Requirements

### REQ-057: 通信制限を準備できない場合の起動拒否

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A62
- verification: unit

起動時に要求されたネットワーク制限を準備できない場合は、起動エラーにしてアプリを実行しない。通信なしの起動や無制限通信への自動移行はしない。

### REQ-058: 制限維持不能時の通信遮断と処理継続

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A63
- verification: unit

起動後に通信制限を維持できなくなった場合は、外向き通信・ホスト側サービスへの接続・待受公開の経路を確実に遮断できる場合に限り、通知して隔離環境内の処理を継続する。遮断を保証できなければ隔離環境を終了する。

### REQ-059: 制限を再構築してからの自動復帰

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A64
- verification: unit

障害により通信を遮断して隔離環境内の処理を継続した場合は、起動時のポリシーで通信制限を再構築し、正常に制限できると確認したら自動復帰して通知する。復帰途中は遮断を維持し、期限切れのDNS許可をそのまま戻さない。切れた接続の再試行はアプリ側に任せる。

### REQ-064: 制限と遮断を保証できない場合の終了結果

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A69
- verification: unit

通信制限を維持できず遮断も保証できないため隔離環境を終了する場合は、kakoi自身の失敗として終了コード125と理由を返す。通常終了時は主コマンドの終了コードをそのまま返す。

### REQ-065: 遮断不能時には終了猶予を適用しない

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A70
- verification: unit

通信制限も遮断も保証できず隔離環境を終了する場合は、通常終了の猶予を適用せず直ちに強制終了する。

## Examples

```gherkin
@id=EX-106 @about=REQ-057 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A62
Scenario: 通信制限の準備に失敗したらアプリを実行しない
  Given ネットワーク制限付きの起動を要求している
  When 要求された通信制限の準備に失敗する
  Then 起動エラーにする
  And アプリを実行しない

@id=EX-107 @about=REQ-058 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A63
Scenario: 外部との通信を遮断できる場合は処理を継続する
  Given 制限維持不能時にも外向き通信とホスト接続と待受公開を確実に遮断できる
  When 実行中に通信制限を維持できなくなる
  Then それらの通信経路を遮断して通知する
  And 隔離環境内の処理は継続する

@id=EX-108 @about=REQ-058 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A63
Scenario: 遮断を保証できない場合は隔離環境を終了する
  Given 通信経路の遮断を保証できない
  When 実行中に通信制限を維持できなくなる
  Then 隔離環境を終了する

@id=EX-109 @about=REQ-059 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A64
Scenario: 通信制限の再構築を確認してから復帰する
  Given 障害で通信を遮断し隔離環境内の処理を続けている
  When 起動時のポリシーで通信制限を再構築し正常に制限できると確認する
  Then 通信を自動復帰して通知する

@id=EX-110 @about=REQ-059 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A64
Scenario: 復帰途中には通信を再開しない
  Given 障害で通信を遮断し通信制限を再構築している
  When 正常に制限できるとの確認がまだ完了していない
  Then 通信の遮断を維持する

@id=EX-111 @about=REQ-059 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A64
Scenario: 期限切れのDNS許可を復帰時に戻さない
  Given 障害前のDNS許可が復帰時点で期限切れである
  When 通信制限を再構築する
  Then その期限切れの許可をそのまま復元しない

@id=EX-120 @about=REQ-064 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A69
Scenario: 制限と遮断の保証を失ったことを終了結果で伝える
  Given 通信制限を維持できず遮断も保証できない
  When そのために隔離環境を終了する
  Then 終了コード125と終了理由を返す

@id=EX-121 @about=REQ-064 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A69
Scenario: 通常終了した主コマンドの結果を変えない
  Given 通信制限を維持できている
  When 主コマンドが終了コード7で通常終了する
  Then 終了コード7を返す

@id=EX-122 @about=REQ-065 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A70
Scenario: 遮断不能の緊急終了で通常の猶予を待たない
  Given 通常終了の猶予に300秒を設定している
  When 通信制限も遮断も保証できず隔離環境を終了する
  Then その猶予を適用せず直ちに強制終了する
```
