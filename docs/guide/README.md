# kakoi リファレンスガイド

kakoi の振る舞いを、利用者と保守者が引けるようにまとめたリファレンス。
kakoi は、層を重ねたポリシーで形を決めた bubblewrap（`bwrap`）の名前空間の中でコマンドを実行し、そのコマンドの終了コードをそのまま返す Linux x86_64 向けのコマンドラインツールである。

## このガイドと IR の関係

契約の正本は `docs/ir/` の IR（kotowari で検査できる形に正規化した仕様）である。
このガイドは IR を人間が読める形に書き直したもので、新しい規則は持たない。
ガイドと IR が食い違う場合は IR が正しい。

各節の見出しの直後には、その節が説明している IR の項目を指す印（`<!-- @kotowari[...] -->`）が HTML コメントとして置かれている。
描画された文書には表示されない。
IR の項目が変わると `kotowari check` がその印に `guide_stale` の通知を出すので、保守者はその節を読み直して IR に合わせる。
印の書き方と見直しの手順は kotowari の guides の説明に従う。

用語の読みと、その意味で使わない言葉はリポジトリ直下の [CONTEXT.md](../../CONTEXT.md) にある。

## 第 1 部 利用者向けリファレンス

kakoi を使う人が、何を指定すると何が起き、何がエラーになるかを引くための部。

| 章 | 扱うこと |
|---|---|
| [kakoi とは](reference/01-overview.md) | 目的、対応環境、対象外 |
| [用語](reference/02-terms.md) | パス、段、計画などの定義 |
| [コマンドライン](reference/03-cli.md) | 書式、オプション、コマンドの解決、`init` |
| [計画の表示](reference/04-plan.md) | `--print-plan` の各形式 |
| [ポリシーファイル](reference/05-policy.md) | 全キーの一覧、形式、パス記法、段の合成、同梱プロファイル |
| [ポリシーを保護する配置](reference/06-policy-placement.md) | 差し替えられる場所にあるポリシーの拒否と回避 |
| [マウント](reference/07-mounts.md) | `rw`、`rw-file`、`rw-copy`、`ro`、`hide`、`scan`、`hide-mounts` |
| [環境変数と認証情報](reference/08-environment.md) | `env`、`secrets`、`git.instead-of` |
| [端末の保護](reference/09-terminal.md) | seccomp による端末の境界 |
| [プロセスと終了コード](reference/10-process.md) | 入れ子、並列、出力、診断の種類と終了コード |
| [シム](reference/11-shim.md) | コマンドを kakoi に通す薄いスクリプト |
| [セットアップスキル](reference/12-setup-skill.md) | 導入を手伝うスキルと承認の境界 |

### ネットワーク

| 章 | 扱うこと |
|---|---|
| [ネットワークモード](reference/network/01-modes.md) | `host`、`none`、`filtered` の違い |
| [通信許可](reference/network/02-allow.md) | 許可の書き方、ポート、DNS 名、ホスト宛て接続 |
| [IP アドレス](reference/network/03-addresses.md) | IP と CIDR の書き方、特殊なアドレス、リンクローカル |
| [DNS による許可](reference/network/04-dns.md) | 応答の検査、許可の寿命、照会の種類と失敗応答 |
| [上流 DNS と処理の上限](reference/network/05-dns-upstream.md) | 上流の設定、ホスト DNS の変更、上限、同時処理 |
| [公開](reference/network/06-publish.md) | ホストから内側のサービスを開く方法 |
| [filtered の監督と終了](reference/network/07-lifecycle.md) | プロセスの終了、障害と復帰、通知 |

## 第 2 部 保守者向けの契約

kakoi を変更する人が守る契約の部。

| 章 | 扱うこと |
|---|---|
| [実行時の境界](maintainer/runtime.md) | 実行時に読むもの、書かないもの、外部依存 |
| [検証の契約](maintainer/verification.md) | テストで何をどう確かめるか |
| [版とリリース](maintainer/release.md) | 版の置き場所と配布物 |
| [公開文書に載せる事項](maintainer/public-docs.md) | 英語の公開文書が説明する内容 |
| [ライブラリの責務と実証条件](maintainer/library.md) | 計画と実行の分離、filtered の提供前の確認 |

## 付録

| 章 | 扱うこと |
|---|---|
| [却下した代替案](appendix/alternatives.md) | 採らなかった設計とその理由（確定要求ではない） |
| [未決事項と既知の問題](appendix/open-issues.md) | 実装裁量、未決の論点、IR 抽出で見つかった問題 |
| [旧仕様の節番号とガイドの対応](appendix/spec-sections.md) | コードや IR に残る「第 N 節」の参照先 |
