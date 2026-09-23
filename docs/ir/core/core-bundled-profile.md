# 同梱プロファイル

既存仕様から継承した本体・補助物の契約。正本は人間向け仕様文書群である。

## Requirements

### REQ-364: 同梱プロファイルの配置と内容
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

examples/profile/default.tomlはWSL2向けに既存codex jailのマウント表を転記し、パスはリンクを実体へ書き直さず元の記法を保つ。rwにworkspace/worktree/git_common_dirと/tmp/kakoi、hideに/tmpと/run/user、.env走査と/mnt以下の9p・drvfsのhide-mountsを含める。READMEから参照する。

### REQ-365: 同梱の認証情報と表示経路の除去
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

同梱のenv.unsetはSSH_AUTH_SOCK、DISPLAY、WAYLAND_DISPLAY、XAUTHORITY、*_TOKEN、*_API_KEY、*_SECRET*、*_PASSWORD、AWS_*、GH_*、GITHUB_*を含む。secretsはconfig_dir/secrets配下を指すコメント1行だけで、有効な項目を含めない。利用者は秘密を渡すときにコメントを外す。

## Examples

```gherkin
@id=EX-668 @about=REQ-364 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 同梱プロファイルの配置と内容・成功
  Given 同梱プロファイルを読む
  When 契約への適合を確認する
  Then 宣言した作業場所と/tmp共有、隠しと走査が含まれる

@id=EX-669 @about=REQ-364 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 同梱プロファイルの配置と内容・反例
  Given 同梱プロファイルを読む
  When 契約への適合を確認する
  Then 転記時にリンクの実体へパスを書き換えることは契約違反である

@id=EX-670 @about=REQ-365 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 同梱の認証情報と表示経路の除去・成功
  Given 初期状態の既定プロファイルを読む
  When 契約への適合を確認する
  Then 指定したunsetがあり有効な秘密項目はない

@id=EX-671 @about=REQ-365 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 同梱の認証情報と表示経路の除去・反例
  Given 初期状態の既定プロファイルを読む
  When 契約への適合を確認する
  Then 初期起動で未配置の秘密の警告を毎回出す設定を同梱することは契約違反である

```
