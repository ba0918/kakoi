# 製品の目的と対応範囲

既存仕様から継承した本体・補助物の契約。この IR が正本である。

## Requirements

### REQ-350: 実行結果と引数の保存
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

kakoiはRust製CLIとして、指定したポリシーとワークスペースでbwrapのマウント名前空間にコマンドを起動する。COMMANDの文字列をargv[0]として保存し、ARGSを追加・変更しない。終了結果はコマンドの終了結果を返す。ただしfilteredの外部終了要求と安全上の故障は既存のネットワーク終了契約を優先する。

### REQ-351: 計画の決定性
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

同じポリシー、ワークスペース、カレントディレクトリ、環境、ファイルシステムの事実からは同じ起動計画を算出する。許可ポリシーは起動時に確定し、アプリから変更・拡張できない。filteredのDNS許可と公開の更新は確定したポリシーの範囲内で行う。

### REQ-352: 対応環境
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

対応環境はLinux x86_64、bwrap 0.9.0以上、root以外の利用者、setuidでないbwrapである。WSL2を含む。x86_64以外へのビルドはコンパイル時に拒否する。32ビットとx32のバイナリは隔離内で実行できない。filteredの追加依存は実証工程で確定し、host/noneに必須としない。

### REQ-353: 対象外の機能
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

0.3ではcgroup資源制限、削除演算子、default.tomlの常時合成、cwdからのポリシー自動探索、bwrapのnew-session、二重隔離、環境変数以外の入れ子検出、git hooks/configの保護、ポリシーのro重ね掛け、separate-git-dirのメインワークツリーの第三の相互リンク形式、initの強制上書きを提供しない。初版のマルチキャスト・ブロードキャストとDoHの扱いは承認済みネットワーク仕様と実証条件を維持する。

## Examples

```gherkin
@id=EX-640 @about=REQ-350 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 実行結果と引数の保存・成功
  Given COMMANDにshを指定した起動
  When 契約への適合を確認する
  Then argv[0]がshのままである

@id=EX-641 @about=REQ-350 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 実行結果と引数の保存・反例
  Given COMMANDにshを指定した起動
  When 契約への適合を確認する
  Then argv[0]を/usr/bin/shに変えることは契約違反である

@id=EX-642 @about=REQ-351 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 計画の決定性・成功
  Given 同じ入力と収集済み事実を二度渡す
  When 契約への適合を確認する
  Then 同じ計画が得られる

@id=EX-643 @about=REQ-351 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 計画の決定性・反例
  Given 同じ入力と収集済み事実を二度渡す
  When 契約への適合を確認する
  Then マウント順が起動ごとに変わることは契約違反である

@id=EX-644 @about=REQ-352 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 対応環境・成功
  Given x86_64以外をビルド対象にする
  When 契約への適合を確認する
  Then コンパイルが失敗する

@id=EX-645 @about=REQ-352 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 対応環境・反例
  Given x86_64以外をビルド対象にする
  When 契約への適合を確認する
  Then aarch64のコンパイル成功を対応済みと扱うことは契約違反である

@id=EX-646 @about=REQ-353 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 対象外の機能・成功
  Given ヘルプとポリシーの固定キーを調べる
  When 契約への適合を確認する
  Then 対象外機能を提供するキーやオプションがない

@id=EX-647 @about=REQ-353 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 対象外の機能・反例
  Given ヘルプとポリシーの固定キーを調べる
  When 契約への適合を確認する
  Then 非公開のキーでdefault.tomlを常時合成することは契約違反である

```
