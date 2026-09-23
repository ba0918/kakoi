# 上流DNSの明示指定

初版の明示指定の範囲を定義する草案。上流の切替条件・待ち時間・通信方式・具体的な記法は未決。

## Requirements

### REQ-111: 明示指定は全問い合わせに共通とする

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A116
- verification: unit

初版のポリシーによる上流DNSの明示指定は、全問い合わせに共通の設定とする。ポリシー内で名前別の上流指定は提供しない。ホストDNS設定を利用する場合の名前別振り分けの継承は維持する。

### REQ-112: 明示した複数上流の範囲内で代替する

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A117
- verification: unit

共通の上流DNSは複数を明示指定でき、問い合わせ先が応答しない場合に指定済みの別候補を使えるようにする。列挙した全候補に同じ名前を問い合わせてよいという指定として扱い、指定外のDNSへは自動で切り替えない。

### REQ-113: 明示した上流を毎回記載順で試す

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A118
- verification: unit

明示した複数上流は、上流問い合わせが必要になるたびに記載順で試す。先の候補が応答しない場合等の切替条件に達したら次へ進む。ホストDNS設定を使う場合の上流選択方式はこの規則の対象外とする。

## Examples

```gherkin
@id=EX-242 @about=REQ-111 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A116
Scenario: 明示指定した共通設定を名前にかかわらず使う
  Given ポリシーで共通の上流DNSを明示指定している
  And 異なる2つの名前の問い合わせが許可されている
  When それぞれの名前について上流問い合わせを開始する
  Then どちらもポリシーで明示指定した共通の上流設定を使う

@id=EX-243 @about=REQ-111 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A116
Scenario: ホスト設定を使う場合は名前別振り分けを維持する
  Given 上流DNSを明示指定せずホストのDNS設定を使っている
  And ホストには名前別の問い合わせ先が設定されている
  When 許可名について上流問い合わせを開始する
  Then ホストの名前別振り分けを使う

@id=EX-244 @about=REQ-112 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A117
Scenario: 応答しない上流の代わりに指定済みの候補を使える
  Given 共通の上流DNSとして複数の候補を明示指定している
  When 問い合わせ先が応答せず代替が必要になる
  Then 明示指定した別候補を使える

@id=EX-245 @about=REQ-112 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A117
Scenario: 全候補が応答しなくても指定外には送らない
  Given 明示指定した上流DNSの全候補が応答しない
  When 許可名を解決しようとする
  Then 明示指定外のDNSへ自動で切り替えない

@id=EX-246 @about=REQ-113 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A118
Scenario: 明示した候補を記載順で試す
  Given 上流DNSとして候補1と候補2をこの順で明示している
  When 上流問い合わせが必要になる
  Then 候補1から試す
  And 候補1が切替条件に達するまでは候補2へ同時に問い合わせない

@id=EX-247 @about=REQ-113 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A118
Scenario: 前回別候補で成功しても次回は先頭から試す
  Given 前回の上流問い合わせは候補1が応答せず候補2で成功した
  When 新たに上流問い合わせが必要になる
  Then 再び候補1から記載順で試す
```
