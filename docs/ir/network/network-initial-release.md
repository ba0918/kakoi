# filtered初版の提供範囲

初版と後続の動的公開を区別する今回の改訂。既存の動的公開要求は将来の要求として保持する。

## Requirements

### REQ-390: 初版を通常のpasta設定で提供する

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A158, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A159
- verification: unit

初版はpastaを前提に、外向きTCP/UDPの制限、DNS由来IPの制限、明示したホスト接続、固定ポート公開、故障時の遮断を提供する。待受の自動検出、競合時の別番号割当、待受寿命に応じた動的公開は後続へ分ける。実験的な動的制御窓口と限定修正版を初版の必須依存にしない。

### REQ-391: 将来の動的公開の責務を分離する

- kind: invariant
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A159
- verification: review

将来の動的公開を追加できるよう公開の判断とpasta固有の操作を分離する。上流の正式対応時期や修正採用を確約された事実として扱わない。上流のUDP削除・再利用問題の解決条件は、その修正を必要とする動的公開の採用に適用する。

### REQ-392: 初版の開始条件と提供条件を分ける

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#D9
- verification: review

実装開始前に上流未修正版で初版の通信・固定公開・全経路遮断・導入経路を確認する。実コードでのDNS・監督・期限・終了の統合は提供前に確認する。過去の限定修正版による部分確認を初版全体の合格へ繰り上げない。

### REQ-393: 固定公開の寿命と起動時の確保

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#D10, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A160
- verification: unit

TCP/UDPの固定公開は内側とホストの番号を起動前に明示する。公開先をホストlocalhostに限定し、内側の指定系統のループバックへ転送する。内側の待受開始や終了に連動せず、隔離環境の終了まで対応を維持する。起動前に実際の確保を確認し、確保失敗ならアプリを起動せず資源を回収する。別番号へ変更しない。復帰も同じ対応を使い、確保できない間は遮断して既存の復帰規則に従う。接続先を通知し、未指定の待受は自動公開しない。

### REQ-394: 固定公開の入力形式

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#D11, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A161
- verification: unit

"[[network.publish]]" に必須の "mode=fixed"、"protocol"、"port"、"host-port" を指定する。protocolはtcpまたはudp、両portは1〜65535の整数。target-familyはipv4またはipv6で既定ipv4、host-familyはipv4またはipv6で既定ipv4。内側はtarget-familyのloopbackへ転送し、host-familyと異なる系統の指定やbothは初版で拒否する。両系統への公開はIPv4用とIPv6用の2項目で明示する。mode省略・範囲・自動割当・mode=dynamicは初版では拒否する。将来も固定設定の意味を維持する。

### REQ-395: 固定公開の合成と設定内競合

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#D11, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A161
- verification: unit

公開リストは段の間で連結し、空配列で下位を消さない。正規化後の同じ項目はまとめる。同じ内側端点への異なる公開設定、同じホスト端点への異なる転送先は設定エラーにする。端点はprotocol・family・portで比較する。host/noneでは形式と設定内競合だけ検査し、実際のポート確保や依存探索はしない。

### REQ-421: 固定公開のホスト側の待受アドレス

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A6
- verification: unit

固定公開のホスト側は、host-familyがipv4なら127.0.0.1だけ、ipv6なら::1だけで待ち受ける。ホストの他のアドレスや全アドレスでは待ち受けない。

### REQ-422: 固定公開の一部だけの成功と終了時の回収

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A6
- verification: unit

複数の固定公開のうち一部だけを確保できた状態ではアプリを起動せず、確保済みの公開も回収する。隔離環境の終了時は、主コマンドの終了ではREQ-061、SIGTERMによる終了要求ではREQ-101の終了処理の中で公開を止め、ホスト側で確保した番号を回収する。

### REQ-423: 固定公開でアプリの標準出力を書き換えない

- kind: prohibition
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A6, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A73
- verification: unit

固定公開を使う場合も、アプリの標準出力を書き換えない。公開の接続先の通知はREQ-068に従って標準エラーに出す。

### REQ-424: pastaのUDPの送信元ポートの制約を公開文書に載せる

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A7
- verification: review
- how_to_verify: "docs/security.md" の filtered の制約を並べた箇所を読み、pasta がUDPの流れごとにホスト側でアプリの送信元ポートと同じ番号を使うこと、その番号がホストで使用中なら流れを作れずデータグラムが通知なく捨てられること、アプリからはタイムアウトに見えることの3点が書かれているかを確かめる。

"docs/security.md" のfilteredの制約に、pastaの次の振る舞いを載せる。pastaはUDPの流れごとに、ホスト側のソケットへアプリ側の送信元ポートと同じ番号を使う。その番号がホストの別のソケットで使用中の場合、pastaはその流れを作れず、同じ送信元からのデータグラムを捨て続ける。アプリからは応答の無いタイムアウトに見え、kakoiは通知しない。

## Examples

```gherkin
@id=EX-722 @about=REQ-390 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A158,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A159
Scenario: 動的公開を初版の前提にしない
  Given 初版の外向き通信制限と固定公開を実装する
  When 必須のpasta機能を選ぶ
  Then 実験的な動的制御窓口と限定修正版を必須にしない

@id=EX-723 @about=REQ-391 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A159
Scenario: 正式対応を待って追加する機能を区別する
  Given 上流の動的公開の修正採用時期が未確定である
  When 動的公開の実装予定を説明する
  Then 正式対応が確約されているとは説明しない

@id=EX-724 @about=REQ-392 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#D9
Scenario: 限定修正版の部分PASSを使って開始条件を省かない
  Given 過去の実証に限定修正版での部分PASSがある
  When 初版の実装開始を判定する
  Then 上流未修正版で初版の通信と固定公開と全経路遮断と導入経路を確認する
```

```gherkin
@id=EX-725 @about=REQ-393 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A160,docs/decision/brainstorm/2026-09-15-kakoi-net.md#D10
Scenario: 内側サービスがまだなくても固定対応を保持する
  Given 起動前に固定公開のホスト側番号を確保した
  When 内側サービスが起動して終了してから再起動する
  Then 隔離環境が動いている間は同じ公開対応を維持する

@id=EX-726 @about=REQ-393 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A160,docs/decision/brainstorm/2026-09-15-kakoi-net.md#D10
Scenario: 固定番号の競合でアプリを起動しない
  Given 指定したホスト側番号を確保できない
  When 固定公開を準備する
  Then 別番号を選ばず起動エラーにする
  And アプリを起動せず確保済み資源を回収する

@id=EX-727 @about=REQ-394 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#D11,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A161
Scenario: 将来の動的指定を固定公開へ読み替えない
  Given 初版の公開項目にmode=dynamicが指定されている
  When 設定を検査する
  Then その指定を拒否する

@id=EX-728 @about=REQ-394 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#D11,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A161
Scenario: 初版でboth指定を拒否する
  Given host-family=bothが指定されている
  When 固定公開の設定を検査する
  Then 設定エラーにする

@id=EX-729 @about=REQ-395 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#D11,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A161
Scenario: 段をまたぐ同じホスト端点への異なる転送先を拒否する
  Given 二つの段に同じホスト端点への異なる転送先が指定されている
  When 公開設定を連結して検査する
  Then 設定エラーにする

@id=EX-730 @about=REQ-395 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#D11,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A161
Scenario: hostモードでは固定公開用ポートを確保しない
  Given hostモードで形式が有効かつ設定内競合のない固定公開指定が残っている
  When 設定を検査する
  Then 公開用ポートの確保もpasta依存探索も行わない

@id=EX-808 @about=REQ-421 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A6
Scenario: IPv4の固定公開はホストの127.0.0.1だけで待ち受ける
  Given host-family=ipv4でTCPのhost-port=8080の固定公開を指定している
  When 隔離環境を起動する
  Then ホストの127.0.0.1のTCP8080番で待ち受ける

@id=EX-809 @about=REQ-421 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A6
Scenario: 固定公開をホストのループバック以外のアドレスで待ち受けない
  Given host-family=ipv4でTCPのhost-port=8080の固定公開を指定し、ホストはLAN側のアドレスも持つ
  When 隔離環境を起動する
  Then ホストのLAN側のアドレスのTCP8080番へは接続できない

@id=EX-810 @about=REQ-422 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A6
Scenario: 一部の固定公開だけが確保できたらアプリを起動しない
  Given TCPのhost-port=8080と8081の固定公開を指定し、ホストの8081番は別のプロセスが使っている
  When 固定公開を準備する
  Then アプリを起動せず起動エラーにする
  And ホストの8080番を待受のまま残さない

@id=EX-811 @about=REQ-422 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A6
Scenario: 終了後にホスト側の公開の番号を残さない
  Given TCPのhost-port=8080の固定公開で隔離環境が動いている
  When 主コマンドが終了してkakoiが終わる
  Then ホストの127.0.0.1のTCP8080番は待受のまま残らない

@id=EX-812 @about=REQ-423 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A6
Scenario: 固定公開があってもアプリの標準出力をそのまま渡す
  Given 固定公開を指定しアプリが標準出力に "hello" を書く
  When 隔離環境でアプリを実行する
  Then kakoiの標準出力は "hello" だけである

@id=EX-813 @about=REQ-423 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A6,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A73
Scenario: 公開の接続先の通知を標準出力に混ぜない
  Given 固定公開を指定しアプリの標準出力を別のコマンドへ渡している
  When 公開の接続先を通知する
  Then その通知を標準出力には出さない

@id=EX-814 @about=REQ-424 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A7
Scenario: UDPの送信元ポートの制約を公開文書で読める
  Given 利用者が "docs/security.md" でfilteredの制約を調べる
  When UDPの通信が応答なく失敗する理由を探す
  Then pastaがアプリの送信元ポートと同じ番号をホスト側で使う制約を読める

@id=EX-815 @about=REQ-424 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A7
Scenario: UDPの送信元ポートの制約が公開文書に無いことは契約違反である
  Given "docs/security.md" のfilteredの制約を読む
  When 契約への適合を確認する
  Then pastaのUDPの送信元ポートの制約が載っていないことは契約違反である
```
