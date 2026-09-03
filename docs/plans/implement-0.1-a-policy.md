# process-wrap 0.1 実装計画 A: 骨組み、コマンドライン、ポリシーの読み込みと合成、変数の導出

この計画は全体像 `docs/plans/implement-0.1.md` を 3 本に割ったうちの A である。方針、再利用の判断、仕様との
対応表の全体、計画全体の「Left to the implementer」「Stop conditions」「Test command」は全体像に
書いてあり、この計画はそれを前提にする。実装者は先に全体像を読むこと。ステップ番号は全体像と
共通で、この計画は ステップ 1〜5 を持つ。

## Goal

crate の骨組みと、ポリシーファイルを読んで合成し、ワークスペースと git の事実から変数を導く核を、純粋なテストが通る状態で作る。bwrap にはまだ触れない。

## Specification

`docs/spec/process-wrap.md`（コミット e0df7b4 で承認済み）。節の書き方は全体像と同じ
（`仕様#3 対応環境` は `## 3. 対応環境`）。用語は `CONTEXT.md` に従う。

## Approach and why

全体像の「Approach and why」のとおり。この計画の成果物は、`cargo build` が通る lib + bin の crate で、`--version`・`--help`・`usage` 診断だけを返すバイナリと、ステップ 3〜5 の核のテスト群である。計画 B と C はこの上に積む。

## Scope of change

- `Cargo.toml`、`Cargo.lock`、`src/lib.rs`、`src/main.rs`、`src/` のコマンドライン解釈・ポリシー読み込み・合成・変数導出・生の git 事実の各モジュール
- `tests/common/mod.rs`、`tests/cli.rs`、`tests/` の該当ファイル
- `PROJECT.md`、`CHANGELOG.md`（Unreleased 節）

全体像の「Scope of change」にある変更しないもの（仕様、用語集、整形と lint の自動化）はここでも
変更しない。

## Step order and prerequisites

1 → 2 → 3 → 4 → 5。ステップ 3 と 5 は 1 の後なら独立に進められる。前提は Rust 1.85 以上、cargo、`git` 2.x（ステップ 5 のテストの準備）。

## Verification map（この計画の分）

| 仕様の節 | ステップ |
|---|---|
| 3 対応環境 | 1 |
| 4.1 文法 | 2 |
| 5.1 形式 | 3 |
| 5.2 パスの書き方（書式） | 3 |
| 5.2 パスの書き方（変数の導出と相互リンク） | 5 |
| 5.3 段の合成、5.5 効かない指定の拒否 | 4 |
| 6.5 作業場所への警告と危険な広さの拒否（位置の拒否） | 5 |
| 13 出力と終了コードの契約（`usage`、`--version`、`--help`、`policy` の読み込み側） | 2、3、4 |
| 14 実行時の境界（読むものの一覧） | 5 |
| 17 版とリリース | 1 |

## Left to the implementer / Stop conditions / Test command

全体像のとおり。この計画に固有の停止条件: なし（全体像のものだけ）。

## Out of scope

ステップ 6 以降（計画 B、C）。マウントの解決、環境の組み立て、bwrap の起動、実機観測、README。全体像の Out of scope もそのまま適用する。

---

## Step 1 — ビルドできる骨組みと対応環境の固定

Purpose: crate を作り、x86_64 の Linux 以外ではコンパイルが失敗する状態と、検証コマンドの置き場を
作る。
Specification: 仕様#3 対応環境、仕様#17 版とリリース。
Prerequisites: Rust 1.85 以上と cargo。
May change: `Cargo.toml`（名前 `process-wrap`、版 `0.1.0`、`rust-version = "1.85"`）、`Cargo.lock`、
`src/lib.rs`、`src/main.rs`、`tests/common/mod.rs`、`PROJECT.md`、`CHANGELOG.md`
（Keep a Changelog の形式で Unreleased 節だけ）。
Done when: `cargo build --locked` が通り、crate のルート（`src/lib.rs`）に `target_arch = "x86_64"`
でなければ `compile_error!` する条件があり（仕様#3 が定めるのはアーキテクチャだけで、OS は
条件に入れない）、`PROJECT.md` に上の 4 本の検証コマンドと `lefthook install` の手順と
`docs/spec/process-wrap.md`・`CONTEXT.md` への参照が書かれ、骨組みの状態で pre-commit の hook
（`lefthook run pre-commit`）と `cargo clippy --all-targets --locked -- -D warnings` が通る。
Shown by: check — `cargo build --locked`、`rg -n 'compile_error' src/lib.rs`（1 行以上）、
`rg -n 'cargo (build|test|fmt|clippy)' PROJECT.md`（4 行）、`rg -n 'lefthook install' PROJECT.md`
（1 行以上）、`rg -n 'docs/spec/process-wrap.md' PROJECT.md`（1 行以上）、`lefthook run pre-commit`
（`src/lib.rs` をステージした状態で、終了コード 0）。
Left to the implementer: なし（計画全体の項目のみ）。
Stop and hand back if: `cargo` がオフラインで `clap` を取れない。

## Step 2 — コマンドラインの解釈と `usage` 診断

Purpose: 仕様#4.1 の文法を受け付け、それ以外を種類 `usage` の診断と終了コード 125 で拒む。
Specification: 仕様#4.1 文法、仕様#13 出力と終了コードの契約（`usage` 行、`--version`/`--help` 行）。
Prerequisites: ステップ 1。
May change: `src/` のコマンドライン解釈モジュール、`src/main.rs`、`tests/cli.rs`。
Done when: 表の 8 オプションが受け付けられ、`--rw`/`--hide` の繰り返しが保持され、`--print-plan`
無しで `COMMAND` が無いとき、未知のオプションのとき、`--` の後ろに何も無いときに
`process-wrap: usage: ` で始まる 1 行を標準エラーに出して 125 で終わり、`--print-plan` があれば
`COMMAND` 無しでも解釈が通り、`--help` と `--version` が標準出力に出て 0 で終わり、`--version` の
出力が `Cargo.toml` の版を含む。オプションの値として受け取ったパス（`--policy-file`、`--workspace`、
`--rw`、`--hide`）は相対ならカレントディレクトリからの相対に直され、`~` と変数は展開されない。
`COMMAND` はこの解決の対象外で、そのまま核に渡る。
Shown by: test — `unknown_option_is_a_usage_diagnostic_with_exit_125`、
`missing_command_without_print_plan_is_a_usage_diagnostic`、`empty_command_after_dashes_is_a_usage_diagnostic`、
`help_prints_to_stdout_and_exits_zero`、`version_prints_the_cargo_version_and_exits_zero`（以上はビルド済み
バイナリ）、`print_plan_form_parses_without_a_command`、
`relative_option_paths_are_resolved_against_the_current_directory`、
`command_is_passed_through_unresolved`（以上は lib の解釈関数）。
Left to the implementer: 解釈結果を核に渡す構造体の形。
Stop and hand back if: `clap` の既定の振る舞い（`--help` の出力先や終了コード）が仕様#13 の表と
食い違い、設定で合わせられない。

## Step 3 — ポリシーファイル 1 つの読み込みと書式の検査

Purpose: TOML を型付きのポリシーに変換し、仕様#5.1 と #5.2 の書式の規則を種類 `policy` の診断に
落とす。まだ合成も展開もしない。
Specification: 仕様#5.1 形式、仕様#5.2 パスの書き方（書式の段落）、仕様#5.3 段の合成（`env.unset` の
ワイルドカードの記述）、仕様#13（`policy` 行）。
Prerequisites: ステップ 1。
May change: `src/` のポリシーのスキーマと読み込みのモジュール、`tests/` の該当ファイル。
Done when: 仕様#5.1 の例がそのまま読め、空のファイルが有効で、未知の固定キー・TOML の解析失敗・
`scan` の `root`/`names` と `hide-mounts` の `under`/`fstype` の欠落または空・相対パス・
`~ユーザー名`・未知の変数名が、それぞれ種類 `policy` の診断になる。`env.set`、`secrets`、
`git.instead-of` は任意のキー名を受け付ける。
Shown by: test — `the_example_policy_file_loads`、`an_empty_policy_file_is_valid`、
`an_unknown_fixed_key_is_a_policy_diagnostic`、`unparsable_toml_is_a_policy_diagnostic`、
`scan_without_names_is_a_policy_diagnostic`、`hide_mounts_with_empty_fstype_is_a_policy_diagnostic`、
`a_relative_path_is_a_policy_diagnostic`、`the_tilde_user_form_is_a_policy_diagnostic`、
`an_unknown_variable_is_a_policy_diagnostic`、`env_set_secrets_and_instead_of_accept_any_key_name`。
Left to the implementer: パスを「絶対 / チルダ / 変数」の 3 種に分けて持つ型の設計。
Stop and hand back if: `toml` の解析器が仕様#5.1 の例の書き方（`rw-file` のようなハイフン付きキー、
`set = { }`）を受け付けない。

## Step 4 — 段の合成

Purpose: 最大 3 つの書かれた段を仕様#5.3 の 3 規則で合成し、`inherit` での `pass`、`env.set` と
`secrets` の同名キー、プロファイルの不在を診断にする。マウント項目の同一性（同じ段の中も段を
またぐものも）は実体のパスで判定するので、ステップ 6 に置く。ここでは項目を段の由来付きで
並べるだけにする。
Specification: 仕様#5.3 段の合成、仕様#5.5 効かない指定の拒否、仕様#4.1 文法（`--profile default` の
扱い、`--policy-file` の不在）、仕様#2 用語（プロファイル、設定ディレクトリ）。
Prerequisites: ステップ 3。テストは `XDG_CONFIG_HOME` を一時ディレクトリに向け、その下に
`process-wrap/profile/` を作ってプロファイルを置く。
May change: `src/` の合成モジュール、`tests/` の該当ファイル。
Done when: リストは連結され（`path-prepend` は上の段が先頭側）、スカラーは上の段が勝ち、テーブルは
キー単位でマージされ、合成後の `inherit` + `pass` が診断になり、下の段 `inherit` + 上の段 `clear` +
`pass` が通り、`env.set` と `secrets` の同名キーが診断になり、`--profile` で指定したファイルの不在と
`--policy-file` の不在が診断になり、`--profile default` の明示が省略時と同じになり、`default` 以外を
指定したとき `default.toml` は読まれない。マウント項目は段の由来を保ったまま次のステップに渡る。
Shown by: test — `lists_concatenate_with_the_upper_layer_appended`、
`path_prepend_puts_the_upper_layer_first`、`scalars_take_the_upper_layer`、
`tables_merge_by_key_with_the_upper_layer_winning`、
`pass_under_inherit_is_a_policy_diagnostic_after_merge`、
`lower_inherit_with_upper_clear_and_pass_is_accepted`、
`the_same_key_in_env_set_and_secrets_is_a_policy_diagnostic`、
`a_missing_profile_file_is_a_policy_diagnostic`、`a_missing_policy_file_target_is_a_policy_diagnostic`、
`an_explicit_default_profile_equals_the_omitted_form`、`a_named_profile_does_not_read_default_toml`。
Left to the implementer: 合成後のポリシーを表す型の形。段の由来は保持する。
Stop and hand back if: なし。

## Step 5 — ワークスペースと git の事実から変数を導く

Purpose: `${workspace}`、`${worktree}`、`${git_common_dir}`、`${config_dir}` の値を、外周が集めた
生の事実から核で導き、相互リンクの検証と位置の拒否を診断にする。
Specification: 仕様#2 用語（ワークスペース、ワークツリー、設定ディレクトリ）、仕様#5.2 パスの
書き方（変数の表と `${git_common_dir}` の規則）、仕様#6.5 作業場所への警告と危険な広さの拒否
（`/`・ホーム・ホームの祖先の段落）、仕様#14 実行時の境界（読むものの一覧）。
Prerequisites: ステップ 1。開発機に `git` 2.x（テストの準備で `git worktree add` と `git submodule`
を使う。製品は git を実行しない）。
May change: `src/` の変数導出モジュール（核）と生の事実を集めるモジュール（外周）、`tests/` の
該当ファイル。
Done when: 核が「ワークスペースの実体、`.git` を持つ最初の祖先（シンボリックリンクの `.git` は
数えない）、`.git` の種類、`gitdir:` の内容、`commondir` の内容、指し返し `gitdir` の内容、`config`
の `core.worktree`、`HOME`、`XDG_CONFIG_HOME`」から 4 変数を返し、次を診断にする: ワークスペースの
不在（`path`）、`HOME` の不在（`env`）、ワークツリーまたはワークスペースが `/`・ホーム・ホームの
祖先（`path`）、リンクされたワークツリーとサブモジュールのどちらの相互リンクも成り立たない `.git`
ファイル（`path`）。`core.worktree` は git の設定ファイルの構文で読む: `[core]` 節の `worktree`
キーの値で、行末までを値とし、両端の二重引用符があれば外す。複数あれば最後のものを使う。
Shown by: test — 核: `a_git_directory_is_the_common_dir`、
`a_linked_worktree_with_a_back_link_yields_the_common_dir`、
`a_linked_worktree_whose_back_link_points_elsewhere_is_a_path_diagnostic`、
`a_submodule_with_core_worktree_pointing_back_yields_its_gitdir`、
`a_git_file_without_any_back_link_is_a_path_diagnostic`、`a_symlinked_dot_git_is_not_a_worktree_marker`、
`a_worktree_at_home_is_a_path_diagnostic`、`a_worktree_at_an_ancestor_of_home_is_a_path_diagnostic`、
`a_missing_workspace_is_a_path_diagnostic`、`a_missing_home_is_an_env_diagnostic`、
`the_config_dir_follows_xdg_config_home`。外周: `raw_git_facts_are_read_from_a_real_linked_worktree`
（テストが `git worktree add` で作った一時リポジトリを読む）、`raw_git_facts_are_read_from_a_real_submodule`
（同じく `git submodule` で作る）。
Left to the implementer: 生の事実を表す構造体の形。
Stop and hand back if: 手元の git が作る `commondir` や `gitdir` の内容が仕様#5.2 の記述と違う形
（例: 絶対パスと相対パスの混在）で、仕様の規則が適用できない。
