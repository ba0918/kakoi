# filteredの入力の誤りの診断

"filtered" の入力の誤りを、見つけた場所によって診断の種類に分け、終了コードを定める。

## Requirements

### REQ-426: ポリシーの読み込みと合成で見つかる誤り

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A8
- verification: unit

ポリシーファイルの読み込みと段の合成で見つかるネットワークの入力の誤りは、種類policyの診断にし、REQ-290に従って終了コード125で終わる。アプリは起動しない。

### REQ-427: filteredの起動の処理で見つかる誤り

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A8
- verification: unit

"filtered" の起動の処理で見つかる誤りは、ポリシーが原因のものも含めて種類bwrapの診断にし、REQ-290に従って終了コード125で終わる。"host-interface" を含む宛先の拒否と、ホストDNSを使うときにホストの名前解決設定に "nameserver" が無い場合もこれに当たる。アプリは起動しない。

## Examples

```gherkin
@id=EX-818 @about=REQ-426 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A8,docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A13
Scenario: 読み込みで見つかる上流の誤りを種類policyで診断する
  Given ポリシーファイルの上流の候補に transport="plain" と tls-name="resolver.example.com" を書いている
  When "filtered" で起動する
  Then 種類policyの診断を出して終了コード125で終わる
  And アプリを起動しない

@id=EX-819 @about=REQ-426 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A8,docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A13
Scenario: 段の合成で初めて見つかる誤りも種類policyで診断する
  Given 下位の段には "plain" の上流だけがあり上位の段には "tls" の上流だけがある
  When "filtered" で起動する
  Then 種類bwrapではなく種類policyの診断を出して終了コード125で終わる

@id=EX-820 @about=REQ-427 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A8
Scenario: host-interfaceを含む宛先を種類bwrapで拒否する
  Given "filtered" の通信許可に destination={ip="fe80::1", host-interface="eth0"} を書いている
  When "filtered" で起動する
  Then 種類bwrapの診断を出して終了コード125で終わる
  And アプリを起動しない

@id=EX-821 @about=REQ-427 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A8
Scenario: ホストの名前解決設定にnameserverが無ければ種類bwrapで診断する
  Given "filtered" でDNS名の許可を使いホストDNSを選び、ホストの名前解決設定に "nameserver" が無い
  When "filtered" で起動する
  Then 種類policyではなく種類bwrapの診断を出して終了コード125で終わる
  And アプリを起動しない
```
