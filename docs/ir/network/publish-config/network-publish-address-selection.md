# 公開対象の待受アドレスと代替番号

初版の対象と番号選択を定義する草案。検出・転送の実証と同時検出時の優先順は未決。

後続の動的公開の要求。A158/A159により初版の対象から分離した。本文の初版は動的公開の初回提供を指す。初版の固定公開はnetwork/network-initial-release.mdで定義する。

- deferred: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A158, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A159

## Requirements

### REQ-136: ループバックと全インターフェースの待受を公開対象にする

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A134
- verification: unit

初版の公開対象は隔離環境内のループバック待受と全インターフェース待受とし、特定の非ループバックIPだけに待ち受けるサービスは対象外とする。ポート・TCP/UDPの許可範囲とホスト側localhost限定は維持する。

### REQ-137: 代替のホスト公開番号を小さい順に試す

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A135
- verification: unit

同番号優先でホスト公開ポートを割り当てられない場合、許可範囲内の空き番号を小さい順に試す。障害復帰時の以前の公開番号優先は維持する。競合状況によって番号は変わり得る。

## Examples

```gherkin
@id=EX-304 @about=REQ-136 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A134
Scenario: 許可範囲内のループバック待受を公開対象にする
  Given 公開を許可したTCPポートで隔離環境内のループバックに待受がある
  When 公開対象を選ぶ
  Then その待受を公開対象とする

@id=EX-305 @about=REQ-136 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A134
Scenario: 非ループバックIPに限定した待受は初版の公開対象外とする
  Given サービスは特定の非ループバックIPだけで待ち受けている
  When 公開対象を選ぶ
  Then その待受は対象外とする

@id=EX-306 @about=REQ-137 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A135
Scenario: 同番号が使えなければ小さい空き番号を優先する
  Given 同番号をホスト公開に使えず許可範囲には18002と18003の空きがある
  When 代替の公開番号を選ぶ
  Then 18002から試す

@id=EX-307 @about=REQ-137 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A135
Scenario: 復帰では小さい番号より以前の公開番号を優先する
  Given 同じ待受の以前の公開番号18003が引き続き許可され空いている
  And 許可範囲内の18002も空いている
  When 障害復帰で再公開する
  Then 以前の18003を優先する

```
