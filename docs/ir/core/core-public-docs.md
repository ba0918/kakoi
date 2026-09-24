# 公開文書に載せる事項

既存仕様から継承した本体・補助物の契約。この IR が正本である。

## Requirements

### REQ-358: 公開文書の導線
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "README.md" が英語で、目的、コンテナやVMの代替でないこと、対応環境、インストール、最短起動、最小ポリシー、仕組み、シム・スキル、保証と限界、詳細文書、対象外の代表例を載せていることを確かめる。READMEから "docs/getting-started.md"、"docs/cli.md"、"docs/policy.md"、"docs/shim.md"、"docs/security.md" へ直接リンクし、これらが英語で詳細を責務ごとに分けて載せていることを確かめる。

公開ドキュメントは英語のREADMEとREADMEから直接リンクするdocs以下の英語文書で構成する。READMEは目的、コンテナやVMの代替でないこと、対応環境、インストール、最短起動、最小ポリシー、仕組み、シム・スキル、保証と限界、詳細文書、対象外の代表例を短く載せる。詳細はgetting-started、cli、policy、shim、securityに責務分離する。

### REQ-359: 既知の隙間の公開
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A1, docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A2
- verification: review
- how_to_verify: "docs/security.md"の既知の隙間の節を表 TBL-160 と突き合わせ、16件がどれも欠けずに意味を変えずに載っていること、READMEからその節へ直接のリンクがあることを人か LLM が確かめる

表 TBL-160 の既知の隙間16件を"docs/security.md"に記載し、READMEからその節へ直接リンクする。16件のどれも省かない。

### REQ-360: 配置保護の説明
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "docs/policy.md" を読み、祖先の改名で保護を迂回できる理由と拒否、dotfilesの設定実体がworktree内のときの下位workspaceとrwのworkspace/git_common_dirの例、下位workspaceはそこで起動し別の場所からリンク越しに指定すると拒否されることと実体の指定による回避、rw内のリンクのhide/root/underは着地先を問わず拒否されること、rw/rw-fileは根の外へ着地すると拒否されること、roの許容と既知の隙間15が説明されていることを確かめる。

"docs/policy.md"は祖先の改名で保護を迂回できる理由と拒否を説明する。dotfilesの設定実体がworktree内なら下位workspaceとrwのworkspace/git_common_dirを例示する。下位workspaceはそこで起動し、別場所からリンク越しに指定すると拒否されること、実体指定による回避を示す。rw内リンクのhide/root/underは着地先を問わず拒否、rw/rw-fileは根の外へ着地すれば拒否、roの許容と既知の隙間15も説明する。

### REQ-361: rw-copyと一時領域の説明
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "docs/policy.md" を読み、rw-copyとhideの初期内容の違い、設定変更をホストへ出さない用途、リンクとモードを保存すること、所有者・時刻・ハードリンクの共有を保存しないこと、上限と失敗、内部のポリシーファイルが配置拒否されないことが説明されていることを確かめる。秘密をconfigのsecrets配下へ案内していること、/tmp/kakoiが利用者かシムが作る共有場所で無ければ飛ばされ/tmpは空であること、/tmpをhideするソケット上の理由が書かれていることを確かめる。

"docs/policy.md"はrw-copyとhideの初期内容の違い、設定変更をホストへ出さない用途、リンク・モードの保存、所有者・時刻・ハードリンク共有を保存しないこと、上限・失敗、内部のポリシーファイルが配置拒否されないことを説明する。秘密はconfigのsecrets配下へ案内し、/tmp/kakoiは利用者かシムが作る共有場所で、なければ飛ばされ/tmpは空であること、/tmpをhideするソケット上の理由を書く。

### REQ-362: インストールの説明順
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "README.md" で配布実行ファイルのインストーラと手動の書庫の手順がソースからのビルドより前にあり、導入直後に組み込みの既定で起動できることとinitとprint-planが最短手順に示されていることを確かめる。"docs/getting-started.md" と "docs/shim.md" が任意の調整としてinit、秘密、シム、gh skill installを案内していることを確かめる。READMEと "docs/getting-started.md" の両方に、初回は/tmpが空でghが未認証だと書かれていることを確かめる。

READMEは配布実行ファイルのインストーラと手動書庫の手順をソースビルドより先に載せる。導入直後に組み込み既定で起動できること、initとprint-planを最短手順に示す。getting-startedとshimでは任意の調整としてinit、秘密、シムとgh skill installを案内する。READMEとgetting-started双方に初回は/tmpが空でghは未認証と書く。

### REQ-363: スキルの隔離外実行の案内
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: READMEとREADMEから直接リンクする詳細文書のセットアップスキルを説明する箇所を読み、スキルを隔離の外で動かすこと、シム導入前・KAKOI_SHIM_OFF=1・kakoiを経由しないCLIの経路が示されていることを確かめる。その間は隔離されないのでスキルに差分の承認を求めさせること、それを守るかはCLIに依存するのでCLI側も承認を求めるモードにすることが案内されていることを確かめる。

セットアップスキルは隔離外で動かすと説明する。シム導入前、KAKOI_SHIM_OFF=1、kakoiを経由しないCLIの経路を示し、その間は隔離されずスキルに差分承認を求めさせること、それを守るかはCLIに依存するためCLI側も承認を求めるモードにすることを案内する。

### REQ-409: 「READMEに書く」の範囲
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A17
- verification: review
- how_to_verify: IRが「READMEに書く」「READMEに載せる」とする事項を列挙し、それぞれをREADMEか、READMEから直接リンクする詳細文書の中で探す。

IRが「READMEに書く」「READMEに載せる」とする事項は、READMEか、READMEから直接リンクする詳細文書のどれかに書くことを指す。READMEの特定の節に載せると定める事項（REQ-408のシムの節の項目）は、その節に載せる。

## Decision tables

### TBL-160: 既知の隙間
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A1, docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A2

| 番号 | 既知の隙間 |
|---|---|
| 1 | "hide" したディレクトリの中のコマンドが「見つかった」と判定される（core-command-resolution.md） |
| 2 | 起動後に現れたファイルは隠れない（core-mounts.md） |
| 3 | 走査で隠したファイルが Git に追跡されていると、隔離の中で空の変更として見える（core-mounts.md） |
| 4 | bind mount やハードリンクによる別の経路は隠せない。"hide" と "rw-copy" は、指定した実体のパスにだけ効く。"rw-copy" の複製元と同じ inode へのハードリンクが "rw" の中にあれば、そこへの書き込みはホストのファイルに届く |
| 5 | マウントするパスと秘密ファイルのパスは bwrap の引数に載り、秘密の値は "/proc" を通してホストから読める（core-environment.md） |
| 6 | 名前が規則に合わないホストの認証情報の環境変数は、"inherit" で隔離に入る（core-environment.md） |
| 7 | "TIOCLINUX"、端末の応答を使った注入、ディスプレイサーバを経由した入力の合成は、seccomp で塞がない（core-terminal.md）。同梱のプロファイルは、これらの経路を "hide" と "unset" で消す |
| 8 | 環境を空にした入れ子と、ホスト側で "KAKOI=1" を立てた起動（core-process.md） |
| 9 | kakoi は起動された環境を信頼する（core-runtime.md）。カレントディレクトリも含む。"rw" の中を通るパスで "cd" し直してから起動すると、隔離の中で差し替えたリンクの先が作業場所になる |
| 10 | "rw" で渡した領域は、利用者がホスト側で後から実行するものの置き場になりうる。".git/hooks" と ".git/config" は、利用者の "git" の操作で自動的に読まれる。隔離の設計では防げず、利用者が差分を見るしかない |
| 11 | 32 ビットと x32 のバイナリは動かない（core-terminal.md） |
| 12 | ワークツリーの ".git" を消すと、次回の起動でワークツリーの導出が祖先のリポジトリに変わりうる（core-mounts.md） |
| 13 | "git init --separate-git-dir" で作ったメインのワークツリー（".git" が通常ファイルで、その指す先に "commondir" も "core.worktree" も無いもの）は、core-policy.md が定める相互リンクのどちらの形にも当たらず、"path" で止まる |
| 14 | core-policy-placement.md の根の項目の検査は、今回の起動で書き込める項目を基準にする。ワークツリーの下位を字面で書いた項目（例: "rw" に "${worktree}" と "~/work/a/b" を並べたポリシー）は、ワークツリーが "~/work" である起動の隔離の中で "~/work/a" をリンクに差し替えられる。次にワークツリーが別の場所になる起動ではその差し替えが見えず、リンクの先に "rw" が付く。項目のパス自身がリンクに差し替えられる形（"~/work/a/b" を差し替え、次の起動で "--workspace ~/work/a/b/inner" を別の場所から与える）も同じである。同じワークツリーから起動すれば止まる。閉じるには前回の起動で書き込めた項目を覚える必要があり、永続する状態を持たないという core-runtime.md の境界に反するので、この隙間は受け入れる。ワークツリーの下位は変数で書く |
| 15 | "rw" の中にある "ro" と "hide" の項目の守りは完全ではない。リンクで書いた "ro" が守るのは、リンク先の実体だけである。隔離の中でリンクを消して同じ名前の通常ファイルを置けば、同じ起動の中で、読む側のパスの内容を変えられる（一時ファイルと rename で保存するエディタも、リンクを置き換える）。リンク先が書き込める項目の外なら、そこは元から読み取り専用であり、"ro" の項目は何も足していない。次の起動では、リンクの差し替えで別の場所が読み取り専用になるか、削除で読み取り専用が外れる。新しく見えるものは、ポリシーを保護する配置の露出する組の検査が止める。リンクで書いた "hide" は、根の項目の検査が 1 回目の起動から止める。リンクでなくても、祖先ディレクトリの改名と、同じパスへの別のファイルの配置で、次の起動が読む内容を変えられる（core-policy-placement.md）。"ro" で守れるのは、エージェントが読む実体の内容が意図せず書き換わらないことまでで、エージェントが別の内容を読むように仕向ける迂回は止められない |
| 16 | シムの雛形は、包む対象のどのオプションが値を取るかを知らないので、写す対象のオプションと同じ名前の語を、別のオプションの値でも位置引数でも、どこにあってもそのオプションとして読む。そのため "--workspace" や "--rw" が、その語に続く語まで広がりうる。例: "codex -m --cd /etc exec" は kakoi に "--workspace /etc" を渡す（core-shim.md）。この誤読は受け入れる |

## Examples

```gherkin
@id=EX-656 @about=REQ-358 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 公開文書の導線・成功
  Given READMEだけから初回起動を進める
  When 契約への適合を確認する
  Then 導入直後にkakoi -- COMMANDを起動でき詳細文書へ直接移れる
@id=EX-657 @about=REQ-358 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 公開文書の導線・反例
  Given READMEだけから初回起動を進める
  When 契約への適合を確認する
  Then 起動前に必ずプロファイル設置が必要だと読めることは契約違反である
@id=EX-658 @about=REQ-359 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1,docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A1,docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A2
Scenario: 既知の隙間の公開・成功
  Given READMEから保証範囲を調べる
  When 契約への適合を確認する
  Then 一回のリンクで16件の具体的な限界を読める
@id=EX-659 @about=REQ-359 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1,docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A1
Scenario: 既知の隙間の公開・反例
  Given READMEから保証範囲を調べる
  When 契約への適合を確認する
  Then 既知の隙間がIRにしかないことは契約違反である
@id=EX-736 @about=REQ-359 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A2
Scenario: シムの雛形が境界を広げうることの公開・成功
  Given 利用者が"docs/security.md"の既知の隙間を読む
  When 契約への適合を確認する
  Then シムの雛形が写すオプションと同じ名前の語で--workspaceや--rwを広げうることが分かる
@id=EX-737 @about=REQ-359 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A2
Scenario: シムの雛形が境界を広げうることの公開・反例
  Given 利用者が"docs/security.md"の既知の隙間を読む
  When 契約への適合を確認する
  Then シムの雛形の写しの誤りが取りこぼしの向きだけだと読めることは契約違反である
@id=EX-660 @about=REQ-360 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 配置保護の説明・成功
  Given 利用者がリンクを使った配置の拒否を調べる
  When 契約への適合を確認する
  Then 拒否理由と該当する実体指定またはcwdでの起動方法が分かる
@id=EX-661 @about=REQ-360 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 配置保護の説明・反例
  Given 利用者がリンクを使った配置の拒否を調べる
  When 契約への適合を確認する
  Then roならリンク元の差し替えも防げると説明することは契約違反である
@id=EX-662 @about=REQ-361 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: rw-copyと一時領域の説明・成功
  Given 設定ファイルだけ隔離内で変更したい利用者が読む
  When 契約への適合を確認する
  Then rw-copyの初期内容と終了時に消える性質が分かる
@id=EX-663 @about=REQ-361 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: rw-copyと一時領域の説明・反例
  Given 設定ファイルだけ隔離内で変更したい利用者が読む
  When 契約への適合を確認する
  Then rw-copyが終了時にホストへ書き戻すと読めることは契約違反である
@id=EX-664 @about=REQ-362 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: インストールの説明順・成功
  Given ビルド環境を持たない利用者がREADMEを読む
  When 契約への適合を確認する
  Then 配布物から導入し組み込み既定で起動できる
@id=EX-665 @about=REQ-362 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: インストールの説明順・反例
  Given ビルド環境を持たない利用者がREADMEを読む
  When 契約への適合を確認する
  Then ソースビルドしか案内しないことは契約違反である
@id=EX-666 @about=REQ-363 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: スキルの隔離外実行の案内・成功
  Given セットアップスキルの実行手順を読む
  When 契約への適合を確認する
  Then 隔離外での操作と承認の境界を認識できる
@id=EX-667 @about=REQ-363 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: スキルの隔離外実行の案内・反例
  Given セットアップスキルの実行手順を読む
  When 契約への適合を確認する
  Then 隔離中の設定編集を当然可能として案内することは契約違反である

@id=EX-775 @about=REQ-409 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A17
Scenario: 「READMEに書く」の範囲・成功
  Given READMEから直接リンクする詳細文書にIRがREADMEに書くとする事項がある
  When 契約への適合を確認する
  Then その事項はREADMEに書いた事項として数える

@id=EX-776 @about=REQ-409 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A17
Scenario: 「READMEに書く」の範囲・反例
  Given IRがREADMEに書くとする事項がREADMEからリンクをたどれない文書にだけある
  When 契約への適合を確認する
  Then 事項がREADMEに書かれていると数えることは契約違反である

```
