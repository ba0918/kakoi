# IPv6リンクローカル宛先の接続口

IPv6リンクローカル宛先を個別に許可するときのネットワーク指定を定義する草案。接続口の名前変更、隔離環境からの経路、広いCIDRで含む場合は未決。

## Requirements

### REQ-052: IPv6リンクローカル宛先の接続口の明示

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A57
- verification: unit

IPv6リンクローカル宛先を個別に許可する場合は、接続先が属するホスト側のインターフェースをポリシーで明示する。未指定は入力エラーにする。この必須指定はIPv6リンクローカル宛先の個別許可に限定し、通常のDNS名や一般のIP指定には求めない。

### REQ-053: ホスト側接続口の入力項目

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A58
- verification: unit

宛先指定の中で "ip" と "host-interface" を別フィールドにする。ホスト側のインターフェース名は "host-interface" に書き、IP欄への%接尾辞による併記は認めない。

### REQ-054: 起動時の接続口の存在確認

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A59
- verification: unit

指定したホスト側インターフェースが起動時に存在しない場合は起動エラーにする。該当ルールを無効化して起動したり、別の接続口へ自動置換したりしない。

### REQ-055: 接続口消失時の影響範囲

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A60
- verification: unit

起動後に指定した接続口が消失した場合は、その接続口に依存する通信を止めて通知する。隔離環境内の処理と他の通信は継続し、別の接続口へ自動で逃がさない。

### REQ-056: 同名の接続口の再出現時の復帰

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A61
- verification: unit

同じ名前の接続口が再出現した場合は、存在を再確認して該当ルールを自動的に有効へ戻す。名前が一致する別の接続口も対象とし、同じ物理機器である保証はしない。切れた接続自体の再試行はアプリ側に任せる。

## Examples

```gherkin
@id=EX-098 @about=REQ-052 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A57
Scenario: ホスト側の接続口を付けてIPv6リンクローカル宛先を許可する
  Given IPv6リンクローカル宛先 "fe80::1" の個別許可にホスト側のインターフェースを指定した
  When 接続口指定の有無を検査する
  Then 接続口の明示条件を満たす

@id=EX-099 @about=REQ-052 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A57
Scenario: IPv6リンクローカル宛先の個別許可で接続口の省略を拒否する
  Given IPv6リンクローカル宛先 "fe80::1" の個別許可にホスト側のインターフェースがない
  When 接続口指定の有無を検査する
  Then 入力エラーにする

@id=EX-100 @about=REQ-052 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A57
Scenario: 一般のIPv4指定には接続口を要求しない
  Given 個別IP "192.168.1.10" の許可にインターフェース指定がない
  When 接続口指定の有無を検査する
  Then インターフェース未指定を理由に入力エラーにしない

@id=EX-101 @about=REQ-053 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A58
Scenario: 接続口を別フィールドから読む
  Given 宛先指定の "ip" が "fe80::1" で "host-interface" が "eth0" である
  When 宛先指定を読み込む
  Then ホスト側の接続口名を "eth0" として受け付ける

@id=EX-102 @about=REQ-053 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A58
Scenario: IP欄への接続口併記を拒否する
  Given IP欄が "fe80::1%eth0" である
  When 宛先指定を読み込む
  Then 入力エラーにする

@id=EX-103 @about=REQ-054 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A59
Scenario: 存在しない接続口を指定したら起動しない
  Given 指定したホスト側インターフェースが存在しない
  When 隔離環境を起動しようとする
  Then 起動エラーにする
  And 別の接続口に自動置換しない

@id=EX-104 @about=REQ-055 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A60
Scenario: 接続口消失で他の処理を終了しない
  Given 指定したホスト側接続口を使うルールが有効である
  When その接続口が消失する
  Then その接続口に依存する通信を止めて通知する
  And 隔離環境内の処理と他の通信は継続する
  And 別の接続口へ自動で逃がさない

@id=EX-105 @about=REQ-056 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A61
Scenario: 同名の接続口が再出現したらルールを復帰する
  Given 接続口の消失によりその接続口に依存する通信を止めている
  When 同じ名前の接続口の存在を再確認する
  Then 該当ルールを自動的に有効へ戻す
  And 切れた接続自体の再試行はアプリ側に任せる
```
