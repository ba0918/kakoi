# IPとCIDRの入力書式

IPv4の表記とCIDRプレフィックス長の入力を定義する草案。インターフェース指定の扱いは未決。

## 要求

### REQ-048: IPv4の10進数4区切りの入力

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A53
- 検証: unit

IPv4入力は0〜255の10進数4個をドットで区切る形式に限る。各数値は0単独を除いて先頭ゼロを認めず、短縮形、16進数形式、1個の整数形式は入力エラーにする。IP欄、CIDRのアドレス部分、IPv4-mappedの末尾IPv4表記に共通適用する。

### REQ-049: CIDRプレフィックス長の範囲と書式

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A54
- 検証: unit

CIDRプレフィックス長はIPv4が0〜32、IPv6が0〜128の10進整数とする。先頭ゼロ（0単独は可）、符号、空白、省略は入力エラーにする。IPv4の/32とIPv6の/128は単一IPの範囲として認める。

### REQ-050: IPv6の標準表記の同一判定

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A55
- 検証: unit

IPv6の標準的な表記の違いは受け付け、数値として同じアドレスなら同じ許可判定とする。英字の大小、各16進数区切りの先頭ゼロ、規則に従う::省略を認める。IPv4の10進数部分の先頭ゼロ禁止とは区別する。

### REQ-051: IP欄とポート欄の分離

- 種類: event_driven
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A56
- 検証: unit

IP欄はアドレス本体だけを受け付け、IPv4/IPv6共通で角括弧・ポート併記・URL・前後の空白を入力エラーにする。ポートは "ports" 欄で指定する。

## 具体例

```gherkin
@id=EX-088 @about=REQ-048 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A53
Scenario: 通常のIPv4表記を受け付ける
  Given IP入力が "192.168.1.10" である
  When IPv4の入力書式を検査する
  Then 受け付ける

@id=EX-089 @about=REQ-048 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A53
Scenario: IPv4の先頭ゼロを拒否する
  Given IP入力が "192.168.001.10" である
  When IPv4の入力書式を検査する
  Then 入力エラーにする

@id=EX-090 @about=REQ-048 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A53
Scenario: IPv4の短縮形を拒否する
  Given IP入力が "127.1" である
  When IPv4の入力書式を検査する
  Then 入力エラーにする

@id=EX-091 @about=REQ-049 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A54
Scenario: 単一IPv6を表す範囲指定を受け付ける
  Given CIDR入力が "fd00::1/128" である
  When プレフィックス長を検査する
  Then 単一IPの範囲として受け付ける

@id=EX-092 @about=REQ-049 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A54
Scenario: プレフィックス長の先頭ゼロを拒否する
  Given CIDR入力が "192.168.1.0/024" である
  When プレフィックス長を検査する
  Then 入力エラーにする

@id=EX-093 @about=REQ-049 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A54
Scenario: IPv4のプレフィックス長の上限超過を拒否する
  Given CIDR入力が "192.168.1.0/33" である
  When プレフィックス長を検査する
  Then 入力エラーにする

@id=EX-094 @about=REQ-050 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A55
Scenario: 展開したIPv6と省略したIPv6を同じ宛先として判定する
  Given "FD00:0000:0000:0000:0000:0000:0000:0001" のTCP・443番を許可している
  When "fd00::1" のTCP・443番への許可を判定する
  Then 同じ宛先の許可に一致すると判定する

@id=EX-095 @about=REQ-050 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A55
Scenario: 不正なゼロ省略は受け付けない
  Given IPv6入力が "fd00::1::2" である
  When IPv6の表記を検査する
  Then 入力エラーにする

@id=EX-096 @about=REQ-051 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A56
Scenario: ポート付きのIPv6をIP欄に受け付けない
  Given IP欄が "[fd00::1]:443" である
  When 入力を検査する
  Then 入力エラーにする

@id=EX-097 @about=REQ-051 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A56
Scenario: 前後の空白を除去して受け付けない
  Given IP欄が " fd00::1 " である
  When 入力を検査する
  Then 入力エラーにする
```
