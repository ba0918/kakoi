# DNS解決失敗の保持と再問い合わせ

失敗時の再問い合わせ抑制を定義する草案。設定記法、キャッシュキー、否定応答・過負荷拒否との区別の細則は未決。

## 要求

### REQ-134: 解決失敗を既定5秒保持する

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A133
- 検証: unit

解決失敗は既定5秒保持し、その間は同じ解決条件の問い合わせに失敗を返して上流への再問い合わせを抑える。保持時間は1〜300秒で設定変更可能とする。

### REQ-135: 同じ上流と通信方式へ同じ問い合わせを繰り返さない

- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-15-kakoi-net.md#A133,docs/decision/brainstorm/2026-09-15-kakoi-net.md#A154
- 検証: unit

1回の解決中、同じ解決条件では同じ上流・通信方式へ同じ問い合わせを繰り返し送らず、応答しなければ次の指定候補へ進む。ホストDNS設定の変更後は別の解決条件として、問い合わせ先IP・通信方式が同じでもやり直しを認める。元の時間・問い合わせ回数の上限は引き継ぎ、設定変更を繰り返してもリセットしない。

## 具体例

```gherkin
@id=EX-301 @about=REQ-134 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A133
Scenario: 失敗直後の同条件の問い合わせを上流へ再送しない
  Given 解決失敗を既定5秒で保持している
  When 2秒後に同じ解決条件の問い合わせを受ける
  Then 上流へ再問い合わせせず失敗を返す

@id=EX-302 @about=REQ-134 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A133
Scenario: 失敗の保持期限後は再び上流へ問い合わせられる
  Given 解決失敗の保持期限が過ぎている
  When 同じ解決条件の許可済み問い合わせを受ける
  Then 過去の失敗の保持だけを理由に上流問い合わせを抑制しない

@id=EX-303 @about=REQ-135 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A133
Scenario: 応答しない上流を同じ方式で繰り返さず次へ進む
  Given 明示上流の候補1が応答せず候補期限に達した
  And 次の指定候補があり全体の上限にも余裕がある
  When 次の問い合わせ先を選ぶ
  Then 候補1へ同じ方式で同じ問い合わせを再送せず候補2へ進む

@id=EX-334 @about=REQ-135 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A154
Scenario: 設定が変われば同じ上流へもやり直せる
  Given ホストDNS設定が変更され問い合わせ先IPと通信方式は変更前と同じである
  When 新しい設定で解決をやり直す
  Then 別の解決条件としてやり直しを認める
  And 元の時間と問い合わせ回数の上限を引き継ぐ

@id=EX-335 @about=REQ-135 @source=docs/decision/brainstorm/2026-09-15-kakoi-net.md#A154
Scenario: 設定変更の繰り返しでも全体上限を延ばさない
  Given 解決中にホストDNS設定の変更が繰り返されている
  When 新しい設定でやり直す
  Then 元の時間と問い合わせ回数の上限をリセットしない

```
