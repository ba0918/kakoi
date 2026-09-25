# 待受公開の失敗と寿命

待受公開の失敗時の影響範囲を定義する草案。再試行設定の可否と試行自体の時間上限、ポート再利用の詳細は未決。隔離環境終了時の方針はnetwork/recovery/network-ownership.mdを参照する。

後続の動的公開の要求。A158/A159により初版の対象から分離した。本文の初版は動的公開の初回提供を指す。初版の固定公開はnetwork/network-initial-release.mdで定義する。

- deferred: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A158, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A159

## Requirements

### REQ-037: 公開ポート不足の影響範囲

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A42
- verification: unit

ホスト側の事前許可範囲に空きがなく公開ポートを確保できない場合は、該当サービスの公開だけを失敗として通知する。隔離環境内の処理と他の公開は継続する。許可範囲外への拡張や既存ホストサービスの停止は行わない。

### REQ-038: 公開ポート不足後の自動再試行

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A43
- verification: unit

公開ポート不足で失敗した場合、サービスの待受が続く間は間隔を置いて自動再試行し、成功時に公開先を通知する。

### REQ-039: TCP待受終了時の確立済み接続の維持

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A44
- verification: unit

TCPの待受終了を検出したら、その公開先からの新規接続の受付を停止する。待受終了だけを理由に確立済み接続を切断せず、最後の応答を送れるように維持する。

### REQ-071: 公開ポート不足の再試行間隔と通知

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A76
- verification: unit

公開ポート不足では失敗後1・2・4・8・16秒、以降30秒待って、待受が続く間だけ再試行する。通知は初回失敗・理由の変化・公開成功に絞り、同じ失敗は繰り返し通知しない。

## Examples

```gherkin
@id=EX-066 @about=REQ-037 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A42
Scenario: ポート不足でも作業中の処理と他の公開を続ける
  Given ホスト側の許可範囲に空きポートがない
  And 隔離環境内で処理が動作しており別のサービスは公開済みである
  When 新しいサービスの公開ポートを確保できない
  Then そのサービスの公開失敗を通知する
  And 隔離環境内の処理と既存の公開は継続する

@id=EX-067 @about=REQ-037 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A42
Scenario: 許可範囲外に空きがあっても公開に使わない
  Given ホスト側の許可範囲内はすべて使用中で範囲外に空きがある
  When サービスの公開ポートを確保しようとする
  Then 許可範囲外のポートを使わず公開失敗を通知する
  And 使用中のホストサービスを停止しない

@id=EX-068 @about=REQ-038 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A43
Scenario: 空きができたら再試行で公開する
  Given 公開ポート不足で失敗したサービスが待受を続けている
  And ホスト側の許可範囲内に空きポートができた
  When 間隔を置いた自動再試行で公開に成功する
  Then 公開先を通知する

@id=EX-069 @about=REQ-038 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A43
Scenario: 待受が終了したサービスの公開は再試行しない
  Given 公開ポート不足で失敗したサービスが待受を終了した
  When 自動再試行の対象を選ぶ
  Then そのサービスを対象としない

@id=EX-070 @about=REQ-039 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A44
Scenario: 待受終了後も確立済み接続の応答を通す
  Given 公開したTCPサービスへの接続が確立している
  When 待受終了を検出する
  Then 新規接続の受付を停止する
  And 確立済み接続の応答を引き続き通す

@id=EX-136 @about=REQ-071 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A76
Scenario: ポート不足が長引いたら30秒間隔で再試行する
  Given サービスの待受が続き公開ポート不足が解消していない
  And 失敗後1・2・4・8・16秒の待ち時間を順に使って再試行した
  When 次の公開にも失敗する
  Then 30秒待って次を試す
  And その後も失敗ごとに30秒待って再試行する

@id=EX-137 @about=REQ-071 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A76
Scenario: 同じポート不足の通知を繰り返さない
  Given 公開ポート不足の初回失敗を通知済みである
  When 再試行が同じ理由で失敗する
  Then 同じ失敗通知を再び出さない

@id=EX-138 @about=REQ-071 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A76
Scenario: 待受が終わったら次の試行を行わない
  Given 公開ポート不足後の再試行まで待っている
  When 対象の待受終了を検出する
  Then その待受の再試行を終了する
```
