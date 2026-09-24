# 版と配布物

既存仕様から継承した本体・補助物の契約。この IR が正本である。

## Requirements

### REQ-386: 版の唯一の典拠
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "Cargo.toml" 以外のソースに版を独立して書いた箇所が無いことを確かめる。"CHANGELOG.md" に各リリースの項目があり、各リリースに版にvを前置したタグがあることを確かめる。リリースしたkakoiの版の出力、"Cargo.toml" の版、タグの版が一致することを確かめる。

版の典拠はCargo.tomlだけに置く。リリースごとにCHANGELOG.mdへ項目を追加し、版にvを前置したタグを付ける。version出力・Cargo.toml・タグの版を一致させる。

### REQ-387: 配布物の契約
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: リリースの配布物を取得し、kakoi-v<版>-x86_64-unknown-linux-musl.tar.gzとその名前に.sha256を足したSHA-256のファイルがあること、書庫の直下に静的リンクの実行ファイルが1つだけあることを確かめる。インストーラが読む書庫名がこの名前と一致することを確かめる。

リリースにはLinux x86_64向け静的リンク実行ファイルを1つ平置きにしたkakoi-v<版>-x86_64-unknown-linux-musl.tar.gzと、その名前に.sha256を足したSHA-256ファイルを添える。インストーラが読む書庫名は契約である。CIと配布物の作成手順はこの仕様に含めない。

## Examples

```gherkin
@id=EX-712 @about=REQ-386 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 版の唯一の典拠・成功
  Given リリースを確認する
  When 契約への適合を確認する
  Then 版の3つの表示が一致する

@id=EX-713 @about=REQ-386 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 版の唯一の典拠・反例
  Given リリースを確認する
  When 契約への適合を確認する
  Then ソースの別の場所で版を独立管理することは契約違反である

@id=EX-714 @about=REQ-387 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 配布物の契約・成功
  Given ビルド道具を持たない利用者が書庫を取得する
  When 契約への適合を確認する
  Then 配布された静的実行ファイルを起動できる

@id=EX-715 @about=REQ-387 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 配布物の契約・反例
  Given ビルド道具を持たない利用者が書庫を取得する
  When 契約への適合を確認する
  Then 動的libcの版を配布先の追加条件にすることは契約違反である

```
