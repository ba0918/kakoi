# 公開文書に載せる事項

既存仕様から継承した本体・補助物の契約。正本は人間向け仕様文書群である。

## 要求

### REQ-358: 公開文書の導線
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: review

公開ドキュメントは英語のREADMEとREADMEから直接リンクするdocs以下の英語文書で構成する。READMEは目的、コンテナやVMの代替でないこと、対応環境、インストール、最短起動、最小ポリシー、仕組み、シム・スキル、保証と限界、詳細文書、対象外の代表例を短く載せる。詳細はgetting-started、cli、policy、shim、securityに責務分離する。

### REQ-359: 既知の隙間の公開
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: review

仕様第16節に継承する既知の隙間15件を"docs/security.md"に記載し、READMEからその節へ直接リンクする。隠れたコマンドの探索、後発ファイル、Git追跡ファイル、別名経路、ホストからの秘密観測、環境の認証情報、端末経路、入れ子、起動環境への信頼、書込領域の後日実行、非対応ABI、Git導出変更、separate-git-dir、別起動の配置変更、ro/hideの限界を省かない。

### REQ-360: 配置保護の説明
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: review

"docs/policy.md"は祖先の改名で保護を迂回できる理由と拒否を説明する。dotfilesの設定実体がworktree内なら下位workspaceとrwのworkspace/git_common_dirを例示する。下位workspaceはそこで起動し、別場所からリンク越しに指定すると拒否されること、実体指定による回避を示す。rw内リンクのhide/root/underは着地先を問わず拒否、rw/rw-fileは根の外へ着地すれば拒否、roの許容と既知の隙間15も説明する。

### REQ-361: rw-copyと一時領域の説明
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: review

"docs/policy.md"はrw-copyとhideの初期内容の違い、設定変更をホストへ出さない用途、リンク・モードの保存、所有者・時刻・ハードリンク共有を保存しないこと、上限・失敗、内部のポリシーファイルが配置拒否されないことを説明する。秘密はconfigのsecrets配下へ案内し、/tmp/kakoiは利用者かシムが作る共有場所で、なければ飛ばされ/tmpは空であること、/tmpをhideするソケット上の理由を書く。

### REQ-362: インストールの説明順
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: review

READMEは配布実行ファイルのインストーラと手動書庫の手順をソースビルドより先に載せる。導入直後に組み込み既定で起動できること、initとprint-planを最短手順に示す。getting-startedとshimでは任意の調整としてinit、秘密、シムとgh skill installを案内する。READMEとgetting-started双方に初回は/tmpが空でghは未認証と書く。

### REQ-363: スキルの隔離外実行の案内
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: review

セットアップスキルは隔離外で動かすと説明する。シム導入前、KAKOI_SHIM_OFF=1、kakoiを経由しないCLIの経路を示し、その間は隔離されずスキルに差分承認を求めさせること、それを守るかはCLIに依存するためCLI側も承認を求めるモードにすることを案内する。

## 具体例

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
@id=EX-658 @about=REQ-359 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 既知の隙間の公開・成功
  Given READMEから保証範囲を調べる
  When 契約への適合を確認する
  Then 一回のリンクで15件の具体的な限界を読める
@id=EX-659 @about=REQ-359 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 既知の隙間の公開・反例
  Given READMEから保証範囲を調べる
  When 契約への適合を確認する
  Then 既知の隙間が仕様書にしかないことは契約違反である
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

```
