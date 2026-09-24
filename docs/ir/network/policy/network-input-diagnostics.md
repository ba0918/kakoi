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
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A8, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A3
- verification: unit

"filtered" の起動の処理で見つかる誤りは種類bwrapの診断にし、REQ-290に従って終了コード125で終わる。ホストDNSを使うときにホストの名前解決設定に "nameserver" が無い場合もこれに当たる。これはポリシーではなくホストの環境が原因の誤りである。アプリは起動しない。

### REQ-428: filteredでのhost-interfaceを含む宛先の拒否

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A2
- verification: unit

合成後のモードが "filtered" で、通信許可に "host-interface" を含む宛先があれば、ポリシーの合成の段階で拒否し、種類policyの診断を出して終了コード125で終わる。"--print-plan" でも計画を表示せずに同じ診断で終わる。合成後のモードが "host" か "none" なら、REQ-084のとおり使わない設定として形式だけを検査して通す。

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

@id=EX-820 @about=REQ-428 @source=docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A1
Scenario: filteredでhost-interfaceを含む宛先を種類policyで拒否する
  Given "filtered" の通信許可に destination={ip="fe80::1", host-interface="eth0"} を書いている
  When "filtered" で起動する
  Then 種類bwrapではなく種類policyの診断を出して終了コード125で終わる
  And アプリを起動しない

@id=EX-822 @about=REQ-428 @source=docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A2
Scenario: 計画の表示でもhost-interfaceを含む宛先を拒否する
  Given "filtered" の通信許可に destination={ip="fe80::1", host-interface="eth0"} を書いている
  When "--print-plan" を付けて起動する
  Then 計画を表示せずに種類policyの診断を出して終了コード125で終わる

@id=EX-823 @about=REQ-428 @source=docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A1
Scenario: hostではhost-interfaceを含む宛先を拒否しない
  Given "host" の通信許可に destination={ip="fe80::1", host-interface="kakoi-absent0"} を書いている
  When "host" で起動する
  Then 診断を出さずにコマンドを起動する

@id=EX-821 @about=REQ-427 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A8,docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A3
Scenario: ホストの名前解決設定にnameserverが無ければ種類bwrapで診断する
  Given "filtered" でDNS名の許可を使いホストDNSを選び、ホストの名前解決設定に "nameserver" が無い
  When "filtered" で起動する
  Then 種類policyではなく種類bwrapの診断を出して終了コード125で終わる
  And アプリを起動しない
```
