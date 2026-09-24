# 明示上流の記法と合成

今回合意した追加・改訂部分の草案。方式の実証は別途必要。

## Requirements

### REQ-146: 明示上流の記法と合成

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A144
- verification: unit

明示DNS上流は "network.dns-upstream" 配列で指定し、下位から上位へ連結する。接続IPとTLSの証明書照合名を分ける。上流指定がなければホストDNSを使う。同じ候補一覧に平文とTLSを混在させない。空配列では下位を消さず、総入替えには別プロファイルを使う。

### REQ-415: 明示上流の候補1件のキー

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A13
- verification: unit

"network.dns-upstream" の候補1件には "transport"、"ip"、"port" を必須とする。"transport" は通常DNSの "plain" かTLSの "tls"、"ip" はIPアドレスそのもの、"port" は1〜65535の整数とする。"tls" では証明書の照合に使う "tls-name" も必須とし、国際化名をASCIIに正規化する。"tls-name" にワイルドカードは指定できない。"plain" に "tls-name" を書いたポリシーは拒否する。

### REQ-416: 明示上流の候補一覧の順序と混在の拒否

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A13
- verification: unit

明示上流の候補一覧は記載順と重複を保つ。1つのポリシーファイルの中でも、段を合成した後の一覧でも、"plain" と "tls" の候補が混在する場合はポリシーを拒否する。

## Examples

```gherkin
@id=EX-324 @about=REQ-146 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A144
Scenario: 上位の空配列は下位の上流を消さない
  Given 下位に明示上流があり上位の上流配列は空である
  When 設定を合成する
  Then 下位の上流は候補に残る

@id=EX-325 @about=REQ-146 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A144
Scenario: 異なる保護方式の混在を受け付けない
  Given 下位にTLS上流があり上位に平文上流がある
  When 設定を合成する
  Then 混在した候補一覧を受け付けない

@id=EX-790 @about=REQ-415 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A13
Scenario: TLS上流の照合名を国際化名から正規化する
  Given 上流の候補に transport="tls"、ip="192.0.2.53"、port=853、tls-name="例え.jp" を書いている
  When ポリシーを読み込む
  Then 候補を受け付け照合名は "xn--r8jz45g.jp" になる

@id=EX-791 @about=REQ-415 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A13
Scenario: 平文の上流に照合名を書いたポリシーを拒否する
  Given 上流の候補に transport="plain"、ip="192.0.2.53"、port=53、tls-name="resolver.example.com" を書いている
  When ポリシーを読み込む
  Then そのポリシーを拒否する

@id=EX-792 @about=REQ-415 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A13
Scenario: 照合名の無いTLS上流を拒否する
  Given 上流の候補に transport="tls"、ip="192.0.2.53"、port=853 だけを書いている
  When ポリシーを読み込む
  Then そのポリシーを拒否する

@id=EX-793 @about=REQ-415 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A13
Scenario: 照合名のワイルドカードとIPでない宛先と範囲外のポートを拒否する
  Given 上流の候補に tls-name="*.example.com"、ip="resolver.example.com"、port=0 のいずれかを書いている
  When ポリシーを読み込む
  Then そのポリシーを拒否する

@id=EX-794 @about=REQ-416 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A13
Scenario: 候補の記載順と重複を保つ
  Given 下位の段に平文上流 192.0.2.1 と 192.0.2.2 がこの順にあり上位の段に 192.0.2.1 がある
  When 段を合成する
  Then 候補一覧は 192.0.2.1、192.0.2.2、192.0.2.1 の順になる

@id=EX-795 @about=REQ-416 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A13
Scenario: 1つのポリシーファイルの中の平文とTLSの混在を拒否する
  Given 1つのポリシーファイルに "plain" の候補と "tls" の候補を書いている
  When ポリシーを読み込む
  Then そのポリシーを拒否する
```
