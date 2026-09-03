# process-wrap 0.1 実装計画 B: マウントの解決、置き場の拒否、環境と秘密と git、引数列、seccomp

この計画は全体像 `docs/plans/implement-0.1.md` を 3 本に割ったうちの B である。方針、再利用の判断、仕様との
対応表の全体、計画全体の「Left to the implementer」「Stop conditions」「Test command」は全体像に
書いてあり、この計画はそれを前提にする。実装者は先に全体像を読むこと。ステップ番号は全体像と
共通で、この計画は ステップ 6〜10 を持つ。

## Goal

計画 A の核の上に、マウント項目の解決と順序、置き場の拒否、環境と秘密と git の書き換え、コマンド解決と bwrap の引数列、seccomp のフィルタ定数を、純粋なテストが通る状態で作る。計画の算出がここで完成する。bwrap にはまだ触れない。

## Specification

`docs/spec/process-wrap.md`（コミット e0df7b4 で承認済み）。節の書き方は全体像と同じ
（`仕様#3 対応環境` は `## 3. 対応環境`）。用語は `CONTEXT.md` に従う。

## Approach and why

全体像の「Approach and why」のとおり。この計画の成果物は、計画（bwrap の引数列、環境、各項目の適用結果、解決したコマンド）を返す核の完成形と、ステップ 6〜10 のテスト群である。隔離の安全性の核心（順序規則、置き場の拒否、秘密の扱い、seccomp）はすべてこの計画にある。レビューはここを厳しく見る。

## Scope of change

- `src/` のマウント解決・判定・環境・秘密・git・コマンド解決・引数列・seccomp の各モジュールと、走査・マウント一覧・パスの事実・秘密ファイルの内容・`PATH` 上の実行ファイルの有無・`bwrap` の有無を集める外周
- `tests/` の該当ファイル

全体像の「Scope of change」にある変更しないもの（仕様、用語集、整形と lint の自動化）はここでも
変更しない。

## Step order and prerequisites

6 → 7 → 8 → 9。ステップ 10（seccomp）は独立で、いつでもよい。前提は計画 A が `main` にマージされていること。

## Verification map（この計画の分）

| 仕様の節 | ステップ |
|---|---|
| 2 用語（計画） | 9 |
| 4.2 コマンドの解決 | 9 |
| 5.2 パスの書き方（展開と値を持たない変数） | 6 |
| 5.4 マウント項目の同一性 | 6 |
| 5.6 ポリシーを書き換えられない置き場 | 7 |
| 6.1 指令の意味、6.2 実体への解決と存在しないパス、6.3 生成される項目、6.4 順序 | 6 |
| 6.5 作業場所への警告と危険な広さの拒否（警告とカレントディレクトリ） | 7 |
| 7 ネットワーク | 9 |
| 8 環境変数、9 認証情報、10 git の URL 書き換え | 8 |
| 11 端末の保護（バイト列） | 10 |
| 13 出力と終了コードの契約（`path`、`secret`、`env`、`bwrap`、`command not found` の生成側） | 6〜9 |
| 14 実行時の境界（固定引数、`bwrap` の所在） | 9 |

## Left to the implementer / Stop conditions / Test command

全体像のとおり。この計画に固有の停止条件: 計画 A の成果物（核の型や関数）が、この計画のステップが前提とする形と食い違うとき。推測で合わせず、報告して止める。

## Out of scope

ステップ 1〜5（計画 A、済み）とステップ 11 以降（計画 C）。核を実際の bwrap につなぐこと、実機観測、README。全体像の Out of scope もそのまま適用する。

---

## Step 6 — 候補パスの展開、同一性、生成される項目、順序

Purpose: 合成後のポリシーと変数から、展開済みの候補パスを作り、外周が集めた事実（存在・種類・
実体、走査結果、マウント一覧、秘密ファイルの有無）と合わせて、同一性を実体で判定し、生成の段を
加え、仕様#6.4 の順序で並べたマウント項目の列を作る。
Specification: 仕様#5.2 パスの書き方（変数展開の効く位置、値を持たない変数）、仕様#5.4 マウント
項目の同一性、仕様#6.1 指令の意味、仕様#6.2 実体への解決と存在しないパス、仕様#6.3 生成される
項目、仕様#6.4 順序、仕様#13 出力と終了コードの契約（`path` 行の `rw` にディレクトリ以外、
`rw-file` にディレクトリ、`usage` 行のコマンドラインでの衝突）。
Prerequisites: ステップ 4、5。
May change: `src/` のマウント解決モジュール（核）と、走査・マウント一覧・パスの事実を集める
モジュール（外周）、`tests/` の該当ファイル。
Done when: 核が候補パスの一覧を返し（外周が事実を集めるため）、事実を与えると次を満たす項目列を
返す: 変数はパスを取る値でだけ展開され `env.set` の値では展開されない、値を持たない変数を含む
項目は存在しないパスと同じく飛ばされる、同じ段の中で同じ実体への同じ指令は 1 つになり違う指令は
`policy`（コマンドライン段なら `usage`）の診断になる、段をまたぐ同じ実体は上の段が置き換える、
存在しない実体は「飛ばした」と理由付きで記録される、走査で名前が一致したディレクトリ以外の
エントリ（読み込んだポリシーファイルを除く。一致したシンボリックリンクは解決先を隠し、解決先が
ディレクトリなら除く）が `hide` になる、走査はシンボリックリンクのディレクトリと `prune` の
ディレクトリに入らない、`hide-mounts` の一致（ワークスペース・ワークツリー・`${git_common_dir}`
そのものを除く）が `hide` になる、存在する秘密ファイルと設定ディレクトリの `secrets/` が `hide` に
なる、生成された項目が書かれた項目を置き換える、祖先が先で兄弟は実体のバイト順に並ぶ、`rw` に
ディレクトリ以外は `rw-file` を案内する説明付きの `path` 診断になり `rw-file` にディレクトリも
`path` の診断になる。仕様#6.4 の表の 5 場面が項目列として再現される。
Shown by: test — `variables_expand_only_in_path_values`、
`an_item_with_a_valueless_variable_is_skipped`、
`the_same_directive_twice_in_one_layer_collapses`、
`conflicting_directives_in_one_layer_are_a_policy_diagnostic`、
`conflicting_directives_on_the_command_line_are_a_usage_diagnostic`、
`an_upper_layer_directive_replaces_the_same_real_path`、
`a_missing_real_path_is_skipped_with_a_reason`、`scan_hides_matching_non_directory_entries`、
`scan_skips_loaded_policy_files`、`scan_hides_the_target_of_a_matching_symlink`、
`scan_skips_a_matching_symlink_to_a_directory`、
`scan_does_not_enter_symlinked_directories`、`hide_mounts_hides_each_matching_mount_target`、
`hide_mounts_leaves_the_workspace_worktree_and_common_dir_alone`、`existing_secret_files_are_hidden`、
`the_config_secrets_directory_is_hidden`、`generated_items_replace_written_items`、
`ancestors_come_before_descendants`、`siblings_are_ordered_by_bytes`、
`rw_on_a_regular_file_is_a_path_diagnostic_naming_rw_file`、`rw_file_on_a_directory_is_a_path_diagnostic`、
仕様#6.4 の表の 5 場面をそれぞれ 1 本ずつ: `a_hidden_ancestor_does_not_hide_an_rw_worktree`、
`an_ro_file_inside_an_rw_directory_stays_read_only`、`a_scanned_env_file_inside_an_rw_worktree_is_hidden`、
`an_rw_subdirectory_shows_through_a_hidden_tmp`、`a_scanned_env_file_under_an_rw_cache_is_hidden`。
外周: `the_mount_list_reads_target_and_fstype_from_mountinfo`（一時ファイルに書いた mountinfo の
写しを読む）、`scan_walks_a_real_tree_with_prune_and_exclude`（一時ディレクトリ）。
Left to the implementer: 走査の探索順、候補パスを集める往復の回数（1 回で済ませるか、走査の起点
だけ先に解決するか）。
Stop and hand back if: `/proc/self/mountinfo` の列の並びが想定と違う環境に当たった。

## Step 7 — 置き場の拒否とカレントディレクトリの規則と警告

Purpose: 仕様#5.6 の 3 種類の拒否、仕様#6.5 のカレントディレクトリの規則と作業場所への警告を、
ステップ 6 の項目列に対して判定する。
Specification: 仕様#5.6 ポリシーを書き換えられない置き場、仕様#6.5 作業場所への警告と危険な広さの
拒否（警告の段落、カレントディレクトリの段落）、仕様#13（`path` 行の該当項目）。
Prerequisites: ステップ 6。
May change: `src/` の判定モジュール（核）、`tests/` の該当ファイル。
Done when: 読み込んだポリシーファイルと設定ディレクトリの与えられたパスの各前置きの実体が `rw` か
`rw-file` の中にあれば、そのパスと `rw` 項目のパスと理由を含む説明の `path` 診断になり、
`path-prepend` の項目が `rw`/`rw-file` の中でも `path` 診断になり、`rw`/`rw-file` の実体が `/`・
ホーム・ホームの祖先でも `path` 診断になり、カレントディレクトリを含む項目のうち順序で最後に
適用されるものが `hide` なら `path` 診断になり、再露出していれば通り、ワークスペースかワークツリーか
その祖先を指す `rw` が無ければ警告が計画に載る。
Shown by: test — `a_policy_file_inside_a_writable_area_is_rejected_with_the_three_reasons`、
`a_policy_file_reached_through_a_symlink_in_a_writable_area_is_rejected`、
`the_config_dir_inside_a_writable_area_is_rejected`、`path_prepend_inside_a_writable_area_is_rejected`、
`rw_on_home_is_rejected_from_any_layer`、`a_cwd_under_a_hide_is_rejected`、
`a_cwd_re_exposed_by_a_descendant_rw_is_accepted`、`no_rw_over_the_workspace_yields_a_warning`、
`rw_over_the_workspace_alone_yields_no_warning`。
Left to the implementer: なし（説明文の並びは計画全体の項目で仕様#20 が委譲）。
Stop and hand back if: なし。

## Step 8 — 環境の組み立て、秘密、git の書き換え

Purpose: 隔離の中の環境の最終形を、仕様#8 の 7 段階のとおりに組み立てる。
Specification: 仕様#8 環境変数、仕様#9 認証情報、仕様#10 git の URL 書き換え、仕様#13（`secret`、
`env` 行）。
Prerequisites: ステップ 7。
May change: `src/` の環境・秘密・git の各モジュール（核）と、秘密ファイルの内容を集める外周、
`tests/` の該当ファイル。
Done when:
- 7 段階の順で環境ができ、`unset` のワイルドカードが効き、`clear` で `PATH` が無く `path-prepend` も
  空なら `PATH` は無いまま。
- 秘密は名前のバイト順に「由来を問わず消してから、ファイルがあれば入れる」で動き、末尾の改行 1 つが
  除かれ、除いた後の 0 バイト・読めない・NUL が `secret` 診断になり、不在が警告になり、値が計画にも
  警告にも診断にも出ない。
- `git.instead-of` に項目があれば、キーのバイト順に `GIT_CONFIG_KEY_n`/`VALUE_n`/`COUNT` を、4 段階目
  までの環境の `GIT_CONFIG_COUNT` の続き（無ければ 0）から足し、既存の組を保持し、数値でない
  `GIT_CONFIG_COUNT` は項目のあるときだけ `env` 診断になる。
Shown by: test — `the_environment_is_assembled_in_the_seven_stages`、`unset_accepts_wildcards`、
`clear_without_path_leaves_path_absent`、`a_secret_removes_the_host_value_before_injecting`、
`a_secret_strips_one_trailing_newline`、`an_empty_secret_file_is_a_secret_diagnostic`、
`a_secret_with_nul_is_a_secret_diagnostic`、`an_unreadable_secret_file_is_a_secret_diagnostic`（テストは
root 以外で実行する前提。root では読めないファイルを作れない）、
`a_missing_secret_file_is_a_warning_without_the_variable`、
`secret_values_never_appear_in_the_plan_or_its_warnings`、`instead_of_entries_continue_the_git_config_count`、
`instead_of_starts_at_zero_when_the_count_is_absent`、
`a_non_numeric_git_config_count_is_an_env_diagnostic_only_with_entries`。
Left to the implementer: 計画の型に「警告の一覧」をどう持つか。
Stop and hand back if: `GIT_CONFIG_*` の命名や `GIT_CONFIG_COUNT` の意味が手元の git の版で違う。

## Step 9 — コマンド解決、bwrap の所在、引数列

Purpose: 計画を完成させる。コマンドの解決、`bwrap` の所在、ネットワーク、固定引数とマウント項目から
なる記号付きの bwrap 引数列。
Specification: 仕様#4.2 コマンドの解決、仕様#7 ネットワーク、仕様#14 実行時の境界（固定引数の
段落、`bwrap` の探索）、仕様#2 用語（計画）、仕様#13（`command not found`、`bwrap` 行）。
Prerequisites: ステップ 8。
May change: `src/` のコマンド解決・引数列のモジュール（核）と、`PATH` 上の実行ファイルの有無と
`bwrap` の有無を集める外周、`tests/` の該当ファイル。
Done when: `/` を含むコマンドは存在と実行可能性を確かめ、含まないコマンドは隔離用 `PATH` を順に探し
（`PATH` が無ければ探さない）、無ければ説明がコマンド名の `command not found` になり、`--print-plan`
で `COMMAND` 省略なら解決せず計画の該当欄が「無し」になり、`bwrap` がホストの `PATH` に無ければ
`bwrap` 診断になり、引数列が仕様#14 の順の固定部分（`host` のときだけ `--share-net`）+ ステップ 6 の
順序付きマウント項目で、`hide` の空ファイルと seccomp のファイル記述子は記号、環境変数を設定する
引数を含まない。
Shown by: test — `a_command_with_a_slash_must_exist_and_be_executable`、
`a_command_is_searched_on_the_isolated_path`、
`an_unresolvable_command_is_a_command_not_found_diagnostic_naming_the_command`、
`print_plan_without_a_command_skips_resolution`、`a_missing_bwrap_is_a_bwrap_diagnostic`、
`fixed_arguments_come_first_in_the_specified_order`、`share_net_is_present_only_for_host_mode`、
`the_argument_list_carries_no_environment_flags`。
Left to the implementer: 記号の表現。
Stop and hand back if: なし。

## Step 10 — seccomp フィルタの定数

Purpose: 仕様#11 の 3 規則だけを持つ BPF プログラムを定数として持ち、仕様が観測条件にしている
「先頭がアーキテクチャの検査」をバイト列の検査で固定する。振る舞いの観測はステップ 15。
Specification: 仕様#11 端末の保護。
Prerequisites: ステップ 2（独立）。
May change: `src/` の seccomp モジュール、`tests/` の該当ファイル。
Done when: 定数のバイト列を BPF の命令（8 バイトごと）として読むと、先頭の命令が `seccomp_data`
のアーキテクチャ欄を読み、次の命令が `AUDIT_ARCH_X86_64` と比較して不一致ならプロセス全体を
終了する返り値に分岐している。`AUDIT_ARCH_X86_64`、`TIOCSTI`、`ioctl` の番号、返り値の定数は
`libc` に無ければ直書きし、出典（カーネルのヘッダ名）を why-not コメントに残す。
Shown by: test — `the_filter_begins_with_an_architecture_check_that_kills_the_process`（命令の
デコードだけを行い、評価器は作らない）。
Left to the implementer: 定数の表現（`[u8]` か `sock_filter` 相当の構造体配列か）。
Stop and hand back if: なし。
