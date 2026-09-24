# 同梱プロファイル

既存仕様から継承した本体・補助物の契約。この IR が正本である。

## Requirements

### REQ-364: 同梱プロファイルの配置と内容
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: examples/profile/default.tomlを読み、既存codex jailのマウント表にあるWSL2向けの項目が転記され、パスがリンクの実体へ書き換えられず元の記法のままであることを突き合わせて確かめる。rwにworkspace、worktree、git_common_dir、/tmp/kakoiが、hideに/tmpと/run/userがあり、.envの走査と/mnt以下の9p・drvfsのhide-mountsがあることを確かめる。READMEからこのファイルを参照していることを確かめる。

examples/profile/default.tomlはWSL2向けに既存codex jailのマウント表を転記し、パスはリンクを実体へ書き直さず元の記法を保つ。rwにworkspace/worktree/git_common_dirと/tmp/kakoi、hideに/tmpと/run/user、.env走査と/mnt以下の9p・drvfsのhide-mountsを含める。READMEから参照する。

### REQ-365: 同梱の認証情報と表示経路の除去
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: examples/profile/default.tomlのenv.unsetにSSH_AUTH_SOCK、DISPLAY、WAYLAND_DISPLAY、XAUTHORITY、*_TOKEN、*_API_KEY、*_SECRET*、*_PASSWORD、AWS_*、GH_*、GITHUB_*がすべてあることを確かめる。secretsがconfig_dir/secrets配下を指すコメント1行だけで有効な項目を含まず、そのコメントを外せば秘密を渡せる形であることを確かめる。

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
