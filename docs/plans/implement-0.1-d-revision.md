# process-wrap 0.1 実装計画 D: 仕様改訂（98a9d5f）の実装

この計画は、計画 A〜C で実装した 0.1 に、仕様の改訂（コミット 98a9d5f、`docs/spec/process-wrap.md`）を
反映する。全体像 `docs/plans/implement-0.1.md` の「Approach and why」「Left to the implementer」
「Stop conditions」「Test command」はそのまま前提にする。全体像の Verification map は改訂前の仕様に
対するもので、改訂で変わった節の割り当ては本計画の表が優先する。実装者は先に全体像と、仕様の 5.6 節・
6.3 節・13 節・14 節を読むこと。ステップ番号は全体像から続けて 17〜25。

## Goal

改訂後の仕様どおりに動く `process-wrap` 0.1: dotfiles へのリンクを `ro` に書いた同梱プロファイルが
そのまま通り、リンクの差し替えや削除で隠しが外れる形は 1 回目の起動から止まり、argv[0]・終了コード
126・記述子の上限・秘密ファイルの CRLF が仕様どおりになり、README が新しい既知の隙間を載せる。

## Specification

`docs/spec/process-wrap.md`（コミット 98a9d5f で改訂。承認済み）。節の書き方は全体像と同じ。用語は
`CONTEXT.md` に従う。仕様の改訂で「変わった規則」の一覧は本計画の Verification map に写してある。

## Approach and why

純粋な関数の境界（`mounts::Candidates` が「何を見るか」を名指し、`mounts::MountFacts` が答えを返す。
`src/startup.rs` が両者を渡す）を保ったまま、検査の入力を広げる。走査の `root`・`hide-mounts` の
`under` の解決、マウント一覧が読めなかったこと、走査で一致したエントリがリンクだったことは、この
継ぎ目の両側に欄を足して純粋な側で判定する。外周（`src/launch.rs`、`src/main.rs`）に足すのは
`--argv0`、記述子の上限、入れ子の 126 だけで、判定は持たせない。exec の方法は今までどおり `execvp`
相当（shebang の無いファイルは `/bin/sh` の下で動く。bwrap も同じ）で、隔離の中と入れ子で exec の成否が
分かれないようにする。診断の文言・計画の表示の整形（項目の由来のラベルを含む）・fixture の配置はテストで
固定しない（契約は 13 節の種類と終了コード、5.6 節・6.3 節が検査ごとに要求する要素だけ）。

## Scope of change

- `src/placement.rs`、`src/mounts.rs`、`src/mount_facts.rs`、`src/mount_list.rs`、`src/scan.rs`、
  `src/plan.rs`、`src/plan_text.rs`、`src/isolated_env.rs`、`src/diagnostic.rs`、`src/startup.rs`、
  `src/launch.rs`、`src/main.rs`、`src/lib.rs`
- `tests/placement.rs`、`tests/mounts.rs`、`tests/plan.rs`、`tests/isolated_env.rs`、`tests/cli.rs`、
  `tests/launch.rs`、`tests/common/mod.rs`、`tests/common/fixture.rs`
- `README.md`、`CHANGELOG.md`

変更しないもの: `docs/spec/`、`CONTEXT.md`、`docs/plans/`、`examples/profile/default.toml`（仕様 16 節
「写しのまま」）、`lefthook.yml`、`.claude/`、`Cargo.toml`（依存の追加は不要のはず。要るなら Reuse
record の判断で `libc` の既存機能を使う）。

## Step order and prerequisites

17 → 18 → 19 → 20 → 21 → 22 → 23 → 24 → 25。17〜22 は純粋な側（`tests/placement.rs`・`tests/mounts.rs`・
`tests/plan.rs`・`tests/isolated_env.rs`）、23 は環境、24 は外周と実機、25 は文書。前提は main が
98a9d5f 以降であること、開発機に `bwrap` 0.9.0 以上、テストを root 以外で実行すること。

## Verification map（この計画の分）

| 仕様の節（改訂で変わった規則） | ステップ |
|---|---|
| 5.6 根の項目の検査の対象（`ro` は対象外、`hide` は残す）、`hide` のリンクを辿る規則と診断の優先 | 17 |
| 5.6 露出する組の規則 2 に生成された `hide` を数える、固定部分（`/`・`/dev`・`/proc`）への着地の拒否と段階 7 の中の位置 | 18 |
| 5.6 走査の `root`・`hide-mounts` の `under` の検査（生成の前、書かれた段の集合、診断の順と要素、`--workspace` の参照の引き継ぎ）、13 節の段階 7 の順 | 19 |
| 5.6 解決の記録（先を読めないリンクの置き場）、15.1 事実（リンクの先） | 19（人の検査） |
| 6.3 生成の順、走査で一致したリンクの先が `ro` の中のときの扱い、マウント一覧が読めないときの `path`、15.1 事実（マウント一覧が読めなかったこと） | 20 |
| 5.2 値の無い変数の飛ばしの表示、8 節 `path-prepend` の実体化と飛ばしの表示 | 21 |
| 15.1 が新たに求めるテスト（5.4 存在しないパスの同一性、5.3 ワイルドカードの意味、10 節の空の `GIT_CONFIG_COUNT`。コードは既に仕様どおり） | 22 |
| 9 節 秘密ファイルの末尾の改行（LF と CR LF） | 23 |
| 1 節・4.2 節・12.1 節 argv[0]、13 節 `command not executable` 126、14 節 `--argv0` と `COMMAND` 省略時の引数列、記述子の soft 上限、15.2 実機観測 | 24 |
| 16 節 既知の隙間 14 の追記と 15、README の書き方と診断の種類、17 節 CHANGELOG | 25 |

改訂で変わったが、コードが既に仕様どおりでテストも既にあり、変更の要らないもの（計画の対象外。
ステップ 24 のレビューで確認だけする）: 13 節の種類 `bwrap` の 2 つの状況（`src/launch.rs` が既に返す）、
14 節のマウント項目の後の `--`、4.2 節・12.1 節の「入れ子では `--print-plan` の有無にかかわらずホストの
`PATH` で探す」、5.6 節の「字面の形とワークスペース由来の形が同じ実体にまとまった項目はワークスペース
由来として扱う」と「明示の `--workspace` が根の項目の検査とリンクの規則の両方に当たれば根の項目の検査の
診断」、5.2 節の「`secrets` の値が値の無い変数のときは 9 節の警告」。

## Left to the implementer（この計画の分）

- 書かれた段だけに 5.4 節の置き換えを適用した集合を純粋な側でどう持つか（`resolve_mounts` の戻り値に
  足すか、検査の側で組み直すか）。
- 走査で一致したエントリがリンクだったことと、リンクの先をどの型で運ぶか。生成由来の「隠さなかった」
  項目と、飛ばした `root`・`under`・`path-prepend` を計画の表示にどう載せるか（`SkippedItem` を広げるか、
  別の欄か。`root` と `under` は同じ受け皿でよい）。
- 記述子の上限の上げ方（`libc` の `getrlimit`/`setrlimit`。既に依存にある `libc` を使う）。
- 診断の説明の文言と、要素の並び。

## Stop conditions（この計画に固有）

- bwrap 0.9.0 の `--argv0` が `-` で始まる値を取れない、または `--` の後ろの `COMMAND` と組み合わせて
  期待どおりに argv[0] を設定しない（2026-09-06 の実測では取れた。食い違えば報告して止める）。
- `setrlimit` で soft 上限を hard 上限まで上げられない環境で `PROJECT.md` の検査 4 本が通らない。
- 純粋な関数の入力に足す事実（走査の `root` の解決、マウント一覧の可否、リンクの先）が、既存の
  `Candidates`／`MountFacts` の継ぎ目で表せず、外周に判定を持ち込まないと実装できないと分かったとき。

## Out of scope

同梱プロファイルの変更、仕様の Status の変更、0.1.0 の tag、README の既知の隙間 1〜13 の文言の
見直し（指摘ファイルに記録のみのものがあるが、この計画では触らない）、全体像の計画の表の書き換え。

---

## Step 17 — 根の項目の検査の対象を絞り、`hide` のリンクを辿る規則を足す

Purpose: `ro` の書かれた項目を根の項目の検査から外して露出する組の検査だけにし、`hide` の書かれた
項目には「項目自身の解決が書き込める項目の中のリンクを辿るなら着地先を問わず `path`」を足す。この規則で
見るのは項目自身の解決だけで、ワークスペース由来の変数で引き継いだ参照は数えない。
Specification: 仕様#5.6 ポリシーを書き換えられない置き場（「それ以外の解決されるパス（根の項目の
検査）」の段落と「露出する組の検査」の段落）、仕様#13 出力と終了コードの契約（`path` の行）。
Prerequisites: main が 98a9d5f 以降。
May change: `src/placement.rs`、`tests/placement.rs`、`tests/plan.rs`。
Done when: `rw = ["~/a", "~/b"]` と `~/a/l -> ~/b/x` で `ro = ["~/a/l/y"]` が通り、`l` をどの項目でもない
場所へ向け直しても通り、`l` を `hide` の中へ向け直すと `path` で止まる。`rw = ["~/proj"]` で
`~/proj/secrets` がどの項目でもない `~/vault` を指す `hide` と、`rw = ["~/a", "~/b"]` で `~/a/l` が
`~/b/creds` を指す `hide` が、どちらも `path` で止まり、診断がその項目のパスと辿ったリンクのパスを含む
（根の項目の検査と両方に当たる前者でも、辿ったリンクのパスが出る）。実体のパスで書いた `rw` の中の
`hide`（`rw = ["~/cache"]`、`hide = ["~/cache/x"]`）は通る。`rw = ["${worktree}", "~/cache"]` と
`hide = ["${worktree}/x"]`（`x` は実体のパス）で、リンク越しの明示の `--workspace` を与えた形は、`hide` の
リンクの規則では止まらない（止まるなら `--workspace` 自身の検査の診断）。既存のテストのうち期待値が
変わるものは書き換える: `tests/placement.rs` の
`a_written_item_resolving_through_a_writable_item_to_outside_every_writable_item_is_rejected` の表の
2 行目（上の段の `ro` を下の段の `hide` の上へ向けた形。露出する組の診断の要素に変える。同じ表の `rw`・
`rw-file` の行はそのまま）、`tests/plan.rs` の
`an_item_reached_through_a_link_between_two_rw_items_is_accepted_until_the_link_moves`（`ro` をどの項目でも
ない場所へ向け直した形。通る期待に変えるか、`rw` の形に変える）。
Shown by: test — `an_ro_written_through_a_writable_link_is_accepted_wherever_it_lands_outside_hide_and_ro`、
`an_ro_written_through_a_writable_link_landing_inside_a_hide_is_rejected`、
`a_hide_whose_resolution_follows_a_link_inside_a_writable_item_is_rejected_wherever_it_lands`、
`a_hide_written_by_real_path_inside_an_rw_item_is_accepted`、
`a_hide_that_fails_both_rules_names_the_followed_link`、
`a_hide_written_with_a_workspace_variable_does_not_count_inherited_references`。
Left to the implementer: 検査の関数の分け方。
Stop and hand back if: なし。

## Step 18 — 生成された `hide` を露出する組の相手に数え、固定部分への着地を拒む

Purpose: 露出する組の規則 2 で「祖先に効いている項目」に生成された `hide`（秘密ファイル、`secrets/`、
走査、`hide-mounts`）を数える。どの指令の項目でも、実体が `/` と同一か `/dev`・`/proc` と同一かその中なら
`path` で止める。固定部分への着地の拒否は、13 節の段階 7 のとおり、露出する組の検査の後、広さの拒否の
前に置く。
Specification: 仕様#5.6（「露出する組の検査」の規則 2 とその後の段落、「固定部分への着地の拒否」、
段階 7 の中の順の段落）、仕様#6.3 生成される項目、仕様#14 実行時の境界（固定部分）、仕様#13（`path` の
行、段階 7）。
Prerequisites: ステップ 17。
May change: `src/placement.rs`、`src/plan.rs`、`tests/placement.rs`、`tests/common/fixture.rs`
（生成の段の事実を与える手段が要れば）。
Done when: `rw = ["~/a"]` と `~/a/link` が `${config_dir}/secrets/other`（`secrets` に書かれていない
ファイル）を指す `ro = ["~/a/link"]` が `path` で止まり、`hide-mounts` で隠した `/mnt/c` の中を指す
`ro` も止まり、診断が無効にする先の生成された `hide` のパスを含む。`hide = ["/"]`、`ro = ["/proc"]`、
`~/a/link` を `/proc` へ向けた `ro = ["~/a/link"]`、`hide = ["/dev/shm"]` が `path` で止まり、
`ro = ["/etc/hosts"]` は通る。
Shown by: test — `an_item_landing_inside_a_generated_hide_is_rejected_as_an_exposing_pair`（`secrets/` と
`hide-mounts` の 2 例）、`an_item_landing_on_the_fixed_mount_targets_is_rejected`（`/` と同一、`/dev` の中、
`/proc` と同一の 3 例）、`an_item_under_root_but_outside_dev_and_proc_is_accepted`。
Left to the implementer: 祖先の探索先を生成の段まで適用済みの集合に替える方法。
Stop and hand back if: なし。

## Step 19 — 走査の `root` と `hide-mounts` の `under` の根の項目の検査

Purpose: 走査の `root` と `hide-mounts` の `under` を根の項目の検査の対象にする。解決が書き込める項目の
中を参照するなら、実体が書き込める項目の実体と同一（マウント点）でなければ `path`。検査は生成の前に、
書かれた段だけに 5.4 節の置き換えを適用した集合で行い、診断は書かれた項目より先に出る（`root` が先、
`under` が次）。診断はその起点のパス、参照した `rw` 項目のパス、理由を含む。ワークスペース由来の変数で
書いた `root`・`under` は明示した `--workspace` の参照を引き継ぐ。あわせて、解決の記録で先を読めない
リンクの置き場を参照に含め、事実にリンクの先を持たせる。
Specification: 仕様#5.6（根の項目の検査の段落、「解決の記録」の段落、`--workspace` の引き継ぎの文）、
仕様#6.3（末尾の例外の段落）、仕様#13（段階 7 の順と、同順の割り振り）、仕様#15.1 純粋な関数の境界
（事実の一覧）。
Prerequisites: ステップ 18。
May change: `src/placement.rs`、`src/mounts.rs`、`src/mount_facts.rs`、`src/plan.rs`、`tests/placement.rs`、
`tests/plan.rs`、`tests/mounts.rs`、`tests/common/fixture.rs`。
Done when: `rw = ["~/a"]` で `~/a/link` がどの項目でもない `~/proj` を指す `root = "~/a/link"` と、
`rw = ["~/b"]` で `root = "~/b/tree"` が `path` で止まり、診断がその起点のパスと参照した `rw` 項目のパスを
含む。`root = "~/b"` と `root = "${worktree}"` は通る。`hide-mounts` の `under` も同じ。生成された `hide` が
`rw` のマウント点と同じ実体に乗る形（`hide-mounts` がその場所を隠す事実を与える）でも、その下位を起点に
する `root` は `path` で止まる（生成前の集合で判定するため）。不正な `root` と不正な書かれた `rw` 項目を
同時に持つポリシーでは `root` の診断が出る。不正な `root` と不正な `under` を同時に持てば `root` の診断が
出る。`rw = ["${worktree}", "~/cache"]`、`root = "${worktree}"` で `--workspace ~/cache/proj`（`proj` が
別の場所へのリンク）を別の場所から与えると止まり、その診断が走査の起点のパスを名指す（`--workspace` の
パスではない。引き継ぎ）。解決の記録の変更（`read_link` の失敗時にもそのリンクの置き場を参照に記録する）
は、発生源を持つテストが作れないので、コードの読み合わせで確認する。
Shown by: test — `a_scan_root_resolving_through_a_writable_item_must_be_a_writable_mount_point`
（拒否 2 例と許容 2 例）、`a_hide_mounts_under_resolving_through_a_writable_item_must_be_a_writable_mount_point`、
`a_scan_root_is_judged_against_the_writable_items_before_generation`、
`a_scan_root_diagnostic_precedes_a_written_item_diagnostic`、`a_scan_root_diagnostic_precedes_a_hide_mounts_under_diagnostic`、
`a_scan_root_written_with_a_workspace_variable_inherits_the_workspace_references`。解決の記録の変更は
レビューでの読み合わせ（テストは要求しない）。
Left to the implementer: 生成前の集合の持ち方、`ExpandedScan`／`ExpandedHideMounts` に変数の由来を
残す方法、`Candidates` に `root`・`under` の解決を足す形。
Stop and hand back if: 生成前の集合を純粋な側で作れない（外周に判定を持ち込む必要がある）と分かった。

## Step 20 — 生成の順、走査のリンク先が `ro` の中のときの扱い、マウント一覧が読めないときの `path`

Purpose: 生成を走査、`hide-mounts`、秘密ファイル、`secrets/` の順にする（実装の順。観測できる差は無い
のでテストは要求しない）。走査で一致したエントリがリンクで、解決先が書かれた段の置き換え後に残る `ro` の
項目の実体と同一かその中なら、その `ro` の解決が書き込める項目（書かれた段の集合）を参照しないなら
隠さず計画に理由付きで表示し、参照するなら `path`（複数あればリンクのパスのバイト順で最初）。
`hide-mounts` が書かれていてマウント一覧が読めなければ `path`。
Specification: 仕様#6.3 生成される項目、仕様#13（`path` の行、段階 7 の生成の括弧）、仕様#15.1
（事実「マウント一覧が読めなかったこと」）、仕様#20（走査の探索順の委譲）。
Prerequisites: ステップ 19。
May change: `src/mounts.rs`、`src/mount_facts.rs`、`src/mount_list.rs`、`src/scan.rs`、`src/plan_text.rs`、
`tests/mounts.rs`、`tests/plan.rs`、`tests/common/fixture.rs`。
Done when: `rw = ["${worktree}"]`、`ro = ["~/.config/opencode/x"]`（解決が書き込める項目を参照しない）
で、ワークツリーの `.env` がそのファイルを指すリンクなら隠さず計画に「隠さなかった」と理由付きで出て
起動が続く。`rw = ["${worktree}", "~/.claude"]`、`ro = ["~/.claude/settings.json"]` で `.env` がそれを
指すリンクなら `path` で止まり、診断がリンクのパスと `ro` の項目のパスを含む。同じ形のリンクが 2 本
あれば、パスのバイト順で先のものが診断に出る。`.env` が `ro` の外の本物のファイルを指すリンクなら
今までどおり隠す。マウント一覧が読めなかった事実を与えた `hide-mounts` 付きのポリシーは `path` で
止まる（`hide-mounts` の無いポリシーにその事実を与える形は、外周がその組み合わせを作らないのでテストに
しない）。既存の `scan_hides_the_target_of_a_matching_symlink` は「`ro` の外を指すリンク」の形に限定して
残す。
Shown by: test — `a_scan_link_into_an_unswappable_ro_item_is_left_visible_with_a_reason`、
`a_scan_link_into_a_swappable_ro_item_is_a_path_diagnostic`、
`the_first_offending_scan_link_in_byte_order_is_named`、`an_unreadable_mount_list_with_hide_mounts_is_a_path_diagnostic`。
Left to the implementer: リンクだったことと先を運ぶ型、「隠さなかった」の受け皿、走査の結果の整列を
どこで行うか。
Stop and hand back if: なし。

## Step 21 — 飛ばしたことの表示（`path-prepend`、値の無い変数）

Purpose: `env.path-prepend` の実体が存在しない項目と、値の無い変数で展開された走査の `root`・`hide-mounts`
の `under`・`path-prepend` を、計画に「飛ばした」と理由付きで表示する。
Specification: 仕様#5.2 パスの書き方（値を持たない変数の段落）、仕様#8 環境変数（6 段階目）、仕様#13
（計画の表示の内容）。
Prerequisites: ステップ 20。
May change: `src/mounts.rs`、`src/plan.rs`、`src/plan_text.rs`、`tests/plan.rs`、`tests/mounts.rs`。
Done when: 存在しない `path-prepend` の項目が `PATH` に入らず、計画に飛ばした理由付きで現れる。
`${git_common_dir}` が値を持たないワークツリーで `root = "${git_common_dir}/x"` と
`under = "${git_common_dir}/m"` を書くと、走査も `hide-mounts` も行われず、計画にどちらも飛ばしたと現れる。
Shown by: test — `a_missing_path_prepend_entry_is_skipped_and_reported`、
`a_scan_root_or_hide_mounts_under_with_a_valueless_variable_is_skipped_and_reported`。
Left to the implementer: 受け皿の型。
Stop and hand back if: なし。

## Step 22 — 15.1 節が新たに求めるテスト（コードは既に仕様どおり）

Purpose: 改訂で 15.1 節のテスト一覧に加わった 3 つを足す。コードは既に仕様どおりなので RED は無い
（テストは最初から通る）。存在しないパスの同一性が展開後の文字列で判定されること、ワイルドカードの `*` が
名前の先頭の `.` に一致し `[abc]` が字面として扱われること、空の `GIT_CONFIG_COUNT` が `env` の診断になる
こと。
Specification: 仕様#5.4 マウント項目の同一性、仕様#5.3 段の合成（ワイルドカードの意味の段落）、
仕様#10 git の URL 書き換え、仕様#15.1 純粋な関数の境界。
Prerequisites: なし（他のステップと独立）。
May change: `tests/mounts.rs`、`tests/isolated_env.rs`。
Done when: 存在しない同じパスを `~/x` と `/home/u/x` の 2 通りで書いた項目が 1 つにまとまる。
`names = ["*"]` の走査が `.env` を隠し、`unset = ["[abc]"]` が名前 `[abc]` だけを消して `a` を消さない。
`GIT_CONFIG_COUNT` が空文字列のホスト環境で `instead-of` の項目があると種類 `env` の診断になる。
Shown by: test — `a_missing_path_written_two_ways_merges_by_its_expanded_text`、
`a_star_matches_a_leading_dot_and_brackets_are_literal`、
`an_empty_git_config_count_is_an_env_diagnostic_with_entries`。
Left to the implementer: なし。
Stop and hand back if: いずれかのテストが最初から通らない（コードが仕様どおりでなかった）。その場合は
製品の修正が要るので報告して止める。

## Step 23 — 秘密ファイルの末尾の改行（LF と CR LF）

Purpose: 秘密ファイルの値から末尾の改行を 1 つ取り除くとき、LF 1 バイトと CR LF の 2 バイトのどちらも
1 つの改行として扱う。
Specification: 仕様#9 認証情報。
Prerequisites: なし（他のステップと独立）。
May change: `src/isolated_env.rs`、`tests/isolated_env.rs`。
Done when: 内容 `v\r\n` の秘密ファイルの値が `v` になり、`v\n` も `v`、`v\r` は `v\r` のまま（CR 単独は
改行でない）、`v\n\n` は `v\n`。既存の `a_secret_strips_one_trailing_newline` の `b"v\r\n" -> "v\r"` の行は
新しい期待値に書き換える。
Shown by: test — `a_secret_strips_one_trailing_lf_or_crlf`。
Left to the implementer: なし。
Stop and hand back if: なし。

## Step 24 — argv[0]、入れ子の 126、`--argv0` と `COMMAND` 省略時の引数列、記述子の上限

Purpose: 包んだコマンドの argv[0] を `COMMAND` に与えた文字列にする（bwrap の固定部分の末尾に
`--argv0 <文字列>`、入れ子は exec で同じ）。入れ子で見つかったコマンドの exec 失敗を種類
`command not executable`、終了コード 126 にする。exec の方法は今までどおり（`execvp` 相当。shebang の
無いファイルは `/bin/sh` の下で動くので失敗にならない。失敗するのは shebang が存在しないインタプリタを
指すスクリプトや、実行できないファイル形式）。`--print-plan` で `COMMAND` を省略した形では `--argv0` と
`--` 以降を引数列に含めない。記述子を作る前に開けるファイル数の soft 上限を hard 上限まで常に上げ、
入れ子では上げない。
Specification: 仕様#1 結果、仕様#4.2 コマンドの解決、仕様#12.1 入れ子、仕様#13（種類の表、状況の表、
段階 10）、仕様#14 実行時の境界（記述子と固定部分）、仕様#15.2 ビルド済みバイナリのテスト。
Prerequisites: ステップ 17〜21（引数列の期待値を持つ `tests/plan.rs` が安定してから）。
May change: `src/plan.rs`、`src/startup.rs`、`src/launch.rs`、`src/main.rs`、`src/diagnostic.rs`、
`tests/plan.rs`、`tests/cli.rs`、`tests/launch.rs`、`tests/common/mod.rs`。
Done when: 引数列の固定部分の末尾に `--argv0 <COMMAND の文字列>` が入り、マウント項目の後の `--` の
後ろに解決したコマンドと `ARGS` が続く（純粋な関数の期待値）。`COMMAND` を省略した `--print-plan` の
引数列に `--argv0` も `--` も無い。隔離の中で `sh -c 'echo $0'` を `COMMAND = sh` で起動すると `sh` が
出て、`-x/tool` のような `-` で始まる相対パスでも argv[0] がその文字列になる。入れ子で同じ起動をしても
`sh` が出る。入れ子で、shebang が存在しないインタプリタを指すスクリプトを起動すると
`process-wrap: command not executable:` で始まる 1 行と終了コード 126 で終わり、通常の起動では bwrap の
失敗がそのまま返る（既存の観測）。soft 上限を 1024 にしたシェルから、`.env` を 1100 個持つワークツリーの
走査付きで起動すると終了コード 0 で終わり、隔離の中の `ulimit -Sn` が hard 上限に等しい。入れ子の起動では
soft 上限が変わらない。
Shown by: test — `the_fixed_arguments_end_with_argv0_and_the_command_follows_the_separator`（`tests/plan.rs`。
既存の `fixed_arguments_come_first_in_the_specified_order` の期待値を更新する形でもよい）、
`print_plan_without_a_command_has_no_argv0_and_no_separator`、
`the_command_sees_the_given_name_as_argv0`（`tests/launch.rs`。`sh` と `-x/tool` の 2 例）、
`a_nested_launch_passes_the_given_name_as_argv0`（`tests/cli.rs`）、
`a_nested_launch_of_a_script_with_a_missing_interpreter_exits_126`（`tests/cli.rs`）、
`a_scan_of_more_hidden_files_than_the_soft_limit_still_launches`（`tests/launch.rs`。`ulimit -n 1024` の
下で 1100 件）、`the_isolated_process_inherits_the_raised_soft_limit`（`tests/launch.rs`）、
`a_nested_launch_leaves_the_soft_limit_unchanged`（`tests/cli.rs`）。
Left to the implementer: `Nested` に `COMMAND` の文字列を持たせる形、`setrlimit` の呼び方。
Stop and hand back if: bwrap の `--argv0` が期待どおりに効かない。`setrlimit` が開発機で失敗する。

## Step 25 — README と CHANGELOG

Purpose: 仕様 16 節が README に載せると定めた新しい内容を書く。既知の隙間 15 と、隙間 14 への追記
（項目のパス自身がリンクに差し替えられる形も同じ）、`rw` の中に置いたリンクのパスを `rw`・`rw-file`・
`hide` に書くと止まりリンク先の実体のパスを書けば通ること、同じ形の `ro` は通るがその守りは隙間 15 の
水準であること、エイリアスのリンク経由の `--workspace` が止まること。5.6 節の説明の節（「Why a policy
file inside a writable area is refused」）を改訂後の規則（`ro` は根の項目の検査の対象外、`hide` は
リンクを辿れば拒否、走査の起点はマウント点）に合わせる。「Exit codes and diagnostics」の種類の列挙に
`command not executable` を足し、入れ子で見つかったコマンドが実行できないときの 126 を書く。CHANGELOG の
0.1.0 の項目に、改訂で変わった利用者に見える振る舞い（argv[0]、126、記述子の上限、CRLF、走査のリンクの
扱い、`ro` の緩和と `hide`・走査の起点の拒否）を足す。
Specification: 仕様#16 公開ドキュメント、仕様#13 出力と終了コードの契約（種類の表）、仕様#17 版と
リリース。
Prerequisites: ステップ 24。
May change: `README.md`、`CHANGELOG.md`。
Done when: README の Known gaps が 15 項目で、14 番目が仕様 16 節の隙間 14 の追記を含み、15 番目が
隙間 15 に対応し、「Why a policy file…」の節が改訂後の 3 つの規則（`ro` は通る、`hide` のリンクは
止まる、走査の起点はマウント点）とリンク先の実体のパスを書く救済を述べ、Exit codes の種類の列挙が仕様
13 節の種類の表と 1 対 1 で対応して 126 を述べ、CHANGELOG の 0.1.0 に上の振る舞いの項目がある。
Shown by: check — 仕様 16 節の README への要求（既知の隙間 15 件の本文と、README に載せると定めた各文）と
仕様 13 節の種類の表を 1 項目ずつ README の見出しまたは段落と突き合わせ、対応の無い項目が 0 であることを
レビューで確認する。`rg -n 'Known gaps' README.md`（1 行）、`rg -c '^[0-9]+\. ' README.md` が 15 以上、
`rg -n 'command not executable' README.md`（1 行以上）、`rg -n 'argv' CHANGELOG.md`（1 行以上）。
Left to the implementer: 英語の文言と構成。
Stop and hand back if: なし。
