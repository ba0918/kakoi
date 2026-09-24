# 旧仕様の節番号とガイドの対応

このガイドの前には、人間向けの仕様が `docs/spec/` にあった。
その仕様は削除したが、コードのコメント、テスト、IR の本文、決定記録には、旧仕様の節番号（「第 5.6 節」「section 13」）での参照が残っている。
この付録は、その番号から、同じ内容を説明しているガイドの章を引くための表である。
削除前の文面は、Git の履歴で `docs/spec/` を見れば読める。

## 本体の節

| 旧節 | 旧節の題 | ガイドの章 |
|---|---|---|
| 1 | 結果 | [kakoi とは](../reference/01-overview.md) |
| 2 | 用語 | [用語](../reference/02-terms.md) |
| 3 | 対応環境 | [kakoi とは](../reference/01-overview.md) |
| 4、4.1 | コマンドラインインターフェース、文法（`init` を含む） | [コマンドライン](../reference/03-cli.md) |
| 4.2 | コマンドの解決 | [コマンドライン](../reference/03-cli.md) |
| 5、5.1 | ポリシーファイルの形式と合成、形式 | [ポリシーファイル](../reference/05-policy.md) |
| 5.2 | パスの書き方 | [ポリシーファイル](../reference/05-policy.md) |
| 5.3 | 段の合成 | [ポリシーファイル](../reference/05-policy.md) |
| 5.4 | マウント項目の同一性 | [ポリシーファイル](../reference/05-policy.md) |
| 5.5 | 効かない指定の拒否 | [ポリシーファイル](../reference/05-policy.md) |
| 5.6 | ポリシーを書き換えられない置き場 | [ポリシーを保護する配置](../reference/06-policy-placement.md) |
| 6、6.1 | マウントの解決、指令の意味 | [マウント](../reference/07-mounts.md) |
| 6.2 | 実体への解決と存在しないパス | [マウント](../reference/07-mounts.md) |
| 6.3 | 生成される項目 | [マウント](../reference/07-mounts.md) |
| 6.4 | 順序 | [マウント](../reference/07-mounts.md) |
| 6.5 | 作業場所への警告と危険な広さの拒否 | [マウント](../reference/07-mounts.md) |
| 7 | ネットワーク | [ネットワークモード](../reference/network/01-modes.md) |
| 8 | 環境変数 | [環境変数と認証情報](../reference/08-environment.md) |
| 9 | 認証情報 | [環境変数と認証情報](../reference/08-environment.md) |
| 10 | git の URL 書き換え | [環境変数と認証情報](../reference/08-environment.md) |
| 11 | 端末の保護 | [端末の保護](../reference/09-terminal.md) |
| 12、12.1、12.2 | 入れ子と並列 | [プロセスと終了コード](../reference/10-process.md) |
| 13 | 出力と終了コードの契約（診断の種類、検査の段階） | [プロセスと終了コード](../reference/10-process.md) |
| 13 | 人が読む計画、ツールが読む計画（JSON） | [計画の表示](../reference/04-plan.md) |
| 14 | 実行時の境界 | [実行時の境界](../maintainer/runtime.md) |
| 15、15.1、15.2、15.3 | 検証の契約 | [検証の契約](../maintainer/verification.md) |
| 16 | 公開ドキュメント（既知の隙間を含む） | [公開文書に載せる事項](../maintainer/public-docs.md) |
| 16 | シム | [シム](../reference/11-shim.md) |
| 16 | セットアップスキル | [セットアップスキル](../reference/12-setup-skill.md) |
| 17 | 版とリリース | [版とリリース](../maintainer/release.md) |
| 18 | 0.3 で作らないもの | [kakoi とは](../reference/01-overview.md) |
| 19 | 却下した代替案 | [却下した代替案](alternatives.md) |
| 20 | 未決と委譲 | [未決事項と既知の問題](open-issues.md) |

第 13 節の「段階 N」（起動前に行う検査の段階と、段階 7 の内訳）は、[プロセスと終了コード](../reference/10-process.md)の表が同じ番号で載せている。

## ネットワークの文書

旧仕様のネットワークの部分は、節番号ではなく文書名で参照されている。

| 旧文書 | ガイドの章 |
|---|---|
| `network/README.md`、`network/policy.md` | [ネットワークモード](../reference/network/01-modes.md)、[通信許可](../reference/network/02-allow.md) |
| `network/dns-input.md`、`network/host.md` | [通信許可](../reference/network/02-allow.md) |
| `network/address.md`、`network/link-local.md` | [IP アドレス](../reference/network/03-addresses.md) |
| `network/dns-trust.md`、`network/dns-lifetime.md`、`network/dns-protocol.md` | [DNS による許可](../reference/network/04-dns.md) |
| `network/dns-upstream-config.md`、`network/dns-host-settings.md`、`network/dns-work-limits.md`、`network/dns-capacity.md` | [上流 DNS と処理の上限](../reference/network/05-dns-upstream.md) |
| `network/initial-release.md`、`network/publish-config.md`、`network/publish-lifetime.md`、`network/publish-target.md` | [公開](../reference/network/06-publish.md) |
| `network/process.md`、`network/recovery.md`、`network/notification.md` | [filtered の監督と終了](../reference/network/07-lifecycle.md) |
| `network/library.md`、`proof-gate.md` | [ライブラリの責務と実証条件](../maintainer/library.md) |
