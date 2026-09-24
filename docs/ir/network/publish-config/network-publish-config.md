# 待受公開の設定形式

公開設定の項目形式を定義する草案。待受アドレスによる絞り込み、通信モードとの整合は未決。

後続の動的公開の要求。A158/A159により初版の対象から分離した。本文の初版は動的公開の初回提供を指す。初版の固定公開はnetwork/network-initial-release.mdで定義する。

## Requirements

### REQ-079: 公開設定の項目とポート範囲

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A84
- verification: unit

公開設定はTOMLの "[[network.publish]]" の各項目に記述する。"protocol" は "tcp" または "udp"、"ports" は隔離環境内の対象ポート範囲、"host-ports" はホスト側の割当許可範囲とし、この3項目は必須とする。"host-family" は "ipv4"・"ipv6"・"both" から選び、省略時は "ipv4" とする。

両ポート欄はnetwork/policy/network-ports.mdで定める文字列配列の記法と入力検査を使い、単一番号・範囲・全ポートを指定できる。両範囲を順番に一対一対応させず、各許可範囲に従って動的に割り当てる。

### REQ-080: 重なる公開設定の統合と競合拒否

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A85
- verification: unit

同じ待受に一致する公開設定が複数ある場合、ホスト側の許可範囲と "host-family" が同じなら重複をまとめる。異なれば起動前の設定エラーにする。同じ待受を設定ごとに複数箇所へ公開しない。

### REQ-081: 公開リストを段の間で連結

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A86
- verification: unit

"network.publish" は段の間で連結し、上の段が追加する。空配列でも下の段の公開設定は消さない。合成後に公開設定の重複・競合を判定する。公開を減らしたい場合は別のプロファイルを使う。

## Examples

```gherkin
@id=EX-155 @about=REQ-079 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A84
Scenario: 内側とホスト側の範囲を別々に指定する
  Given network.publishの項目にprotocol="tcp"とports=["8000-8010"]とhost-ports=["18000-18010"]を指定している
  When 公開設定を解釈する
  Then 内側のTCPの8000〜8010番を公開対象とする
  And ホストの18000〜18010番を割当許可範囲とする
  And host-familyはipv4として扱う

@id=EX-156 @about=REQ-079 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A84
Scenario: ホスト側の範囲省略を拒否する
  Given network.publishの項目にprotocol="tcp"とports=["8000"]だけを指定している
  When 公開設定を検査する
  Then host-portsの欠落を設定エラーにする

@id=EX-157 @about=REQ-079 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A84
Scenario: ポート範囲の順番を固定対応と解釈しない
  Given ports=["8000-8010"]とhost-ports=["18000-18010"]でTCPを公開する
  And 内側の8001番に待受がありホストの18001番は使用中で18002番は空いている
  When 公開ポートを選ぶ
  Then 8001番の転送先を18001番に固定せず許可範囲内の空きを選ぶ

@id=EX-158 @about=REQ-079 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A84,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A77
Scenario: UDPとIPv6の公開を指定する
  Given network.publishの項目にprotocol="udp"とports=["9000"]とhost-ports=["19000"]とhost-family="ipv6"を指定している
  When 公開設定を解釈する
  Then 内側のUDP9000番を対象としてホストの::1の19000番での公開を要求する

@id=EX-159 @about=REQ-080 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A85
Scenario: 同じ公開先条件の重複をまとめる
  Given TCP8000番に一致する2つの公開設定がある
  And 両方のhost-portsは["18000"]でhost-familyはipv4である
  When 公開設定を検査する
  Then 重複をまとめて1つの公開として扱う

@id=EX-160 @about=REQ-080 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A85
Scenario: 同じ対象に異なるホスト範囲を指定したら起動前に拒否する
  Given TCP8000番に一致する2つの公開設定がある
  And 片方のhost-portsは["18000"]でもう片方は["19000"]である
  When 公開設定を検査する
  Then 起動前の設定エラーにする

@id=EX-161 @about=REQ-080 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A85
Scenario: 同じ対象のhost-familyが異なる場合も拒否する
  Given TCP8000番に一致する2つの公開設定がありホスト側の許可範囲は同じである
  And 片方のhost-familyはipv4でもう片方はipv6である
  When 公開設定を検査する
  Then 起動前の設定エラーにする

@id=EX-162 @about=REQ-081 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A86
Scenario: 追加ファイルの公開対象を足す
  Given プロファイルにTCP8000番の公開設定がある
  And 上の段の追加ファイルにTCP9000番の公開設定がある
  When 設定を合成する
  Then 両方を公開対象として残す

@id=EX-163 @about=REQ-081 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A86
Scenario: 空配列で下の段の公開を消さない
  Given 下の段に公開設定がある
  And 上の段のnetwork.publishは空配列である
  When 設定を合成する
  Then 下の段の公開設定を維持する

@id=EX-164 @about=REQ-081 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A86,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A85
Scenario: 段をまたぐ競合も起動前に拒否する
  Given 下の段でTCP8000番の公開先をホスト18000番に指定している
  And 上の段で同じTCP8000番の公開先をホスト19000番に指定している
  When 設定を合成して検査する
  Then 上書きせず公開設定の競合として起動前にエラーにする
```
