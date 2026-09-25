# 待受終了後の公開番号

待受終了と新たな待受開始の扱いを定義する草案。OSによるポート再利用の制約と待受終了の検出方法は未決。

後続の動的公開の要求。A158/A159により初版の対象から分離した。本文の初版は動的公開の初回提供を指す。初版の固定公開はnetwork/network-initial-release.mdで定義する。

- deferred: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A158, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A159

## Requirements

### REQ-078: 待受終了後に再起動用の予約を残さない

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A83
- verification: unit

待受終了後は再起動用のホスト公開番号の予約を残さず解放する。待受が再び現れたら新規公開として番号を選ぶ。待受終了時に新規受付を停止し、確立済みTCP接続は維持する。

## Examples

```gherkin
@id=EX-152 @about=REQ-078 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A83
Scenario: 待受終了後に再起動用の番号予約を残さない
  Given サービスの待受をホストで公開している
  When その待受の終了を検出する
  Then 新規受付を停止する
  And 再起動用の公開番号の予約を残さず解放する

@id=EX-153 @about=REQ-078 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A83
Scenario: 再開した待受は新規公開として番号を選ぶ
  Given 以前の待受の終了を検出してその公開番号を解放済みである
  When 同じ受付先に新たな公開対象の待受が現れる
  Then 新規公開の規則でホスト公開番号を選ぶ

@id=EX-154 @about=REQ-078 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A83
Scenario: 番号予約の解放を理由に既存TCP接続を切らない
  Given 公開したTCP待受への接続が確立している
  When その待受が終了して再起動用の番号予約を残さず片付ける
  Then 確立済みTCP接続を維持する
```
