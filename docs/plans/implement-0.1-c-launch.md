# process-wrap 0.1 実装計画 C: 外周の結線、決定性、同梱プロファイル、README、実機観測、移行確認

この計画は全体像 `docs/plans/implement-0.1.md` を 3 本に割ったうちの C である。方針、再利用の判断、仕様との
対応表の全体、計画全体の「Left to the implementer」「Stop conditions」「Test command」は全体像に
書いてあり、この計画はそれを前提にする。実装者は先に全体像を読むこと。ステップ番号は全体像と
共通で、この計画は ステップ 11〜16 を持つ。

## Goal

計画 A と B の核を実際の bwrap につなぎ、仕様 13 節の表と隔離の中の振る舞いを実機で観測し、同梱プロファイルと README を揃え、最後に現行 jail との比較を人が行う。

## Specification

`docs/spec/process-wrap.md`（コミット e0df7b4 で承認、1142262 で改訂済み）。節の書き方は全体像と同じ
（`仕様#3 対応環境` は `## 3. 対応環境`）。用語は `CONTEXT.md` に従う。

## Approach and why

全体像の「Approach and why」のとおり。この計画の成果物は、動く `process-wrap` 0.1 と、実際の bwrap で通るテスト群と、README と `examples/profile/default.toml` である。

## Scope of change

- `src/main.rs`、`src/` の起動モジュール（外周）
- `tests/cli.rs`、`tests/` の統合テストファイル、`tests/common/mod.rs`
- `examples/profile/default.toml`、`README.md`、`CHANGELOG.md`

全体像の「Scope of change」にある変更しないもの（仕様、用語集、整形と lint の自動化）はここでも
変更しない。

## Step order and prerequisites

11 → 12 → 13 → 14 → 15 → 16。ステップ 12 と 15 は観測だけを行い、不一致は該当する前の計画のステップに差し戻す。前提は計画 B が `main` にマージされていること、開発機に `bwrap` 0.9.0 以上があること、テストを root 以外で実行すること。

## Verification map（この計画の分）

| 仕様の節 | ステップ |
|---|---|
| 1 結果 | 12（決定性）、15（引数の透過） |
| 4.2 コマンドの解決（隠したディレクトリの中のコマンド） | 15 |
| 6.1 指令の意味（rename、ソケット）、6.4 順序（実機） | 15 |
| 6.5 作業場所への警告と危険な広さの拒否（観測条件） | 15 |
| 7 ネットワーク（実機） | 15 |
| 8 環境変数、9 認証情報、10 git の URL 書き換え（実機、秘密の値の非表示） | 11、15 |
| 11 端末の保護（実機） | 15 |
| 12.1 入れ子、12.2 並列 | 11、15 |
| 13 出力と終了コードの契約（表の全行） | 11、15 |
| 14 実行時の境界（ファイル記述子、exec、ファイルを書かない） | 11、15 |
| 15.2 ビルド済みバイナリのテスト | 15 |
| 15.3 移行の確認 | 16（人が行う） |
| 16 公開ドキュメント | 13、14 |
| 17 版とリリース | 14 |

## Left to the implementer / Stop conditions / Test command

全体像のとおり。この計画に固有の停止条件: 計画 A・B の成果物が、この計画のステップが前提とする形と食い違うとき。`bwrap` が無いとき。実測が仕様の記述と食い違うとき（報告して止める）。

## Out of scope

ステップ 1〜10（計画 A と B、済み）。0.1.0 の tag（リリース時の操作）。全体像の Out of scope もそのまま適用する。

---

## Step 11 — 外周の結線: 診断の出力、計画の表示、入れ子、bwrap の起動

Purpose: 核の出力を実際の振る舞いにする。診断と警告の標準エラー出力と終了コード、`--print-plan` の
表示、入れ子の分岐、ファイル記述子の準備と exec。
Specification: 仕様#13 出力と終了コードの契約（検査順序の段階 2 の後の入れ子の分岐と段階 9、制御文字の逃がしの警告と計画の表示への適用）、仕様#12.1 入れ子、仕様#4.2 コマンドの解決（入れ子でのホストの `PATH`）、仕様#14 実行時の境界（ファイル
記述子、exec）、仕様#8 環境変数（環境を bwrap に引き継がせる）。
Prerequisites: ステップ 2〜10。
May change: `src/main.rs`、`src/` の起動モジュール（外周）、`tests/cli.rs`。
Done when: 診断が `process-wrap: <種類>: <説明>` の 1 行で標準エラーに出て 125（`command not found`
は 127）で終わり、警告が `process-wrap: warning: ` で始まり、`--print-plan` が診断の無いとき計画を
標準出力に出して 0 で終わり診断のあるときは計画を出さず、`PROCESS_WRAP=1` のとき `--print-plan`
以外は仕様#13 の段階 3〜8 を飛ばし（カレントディレクトリとホームの検査も行わない）、ポリシーを読まず環境を変えず、受け取ったホストの `PATH` でコマンドを解決して見つからなければ `command not found` の 127 で終わり、見つかれば警告 1 行の後にコマンドを exec し、警告と計画の表示に埋め込む値の制御文字が計画 A2 の逃がしの部品で逃がされ、`PROCESS_WRAP=1` での
`--print-plan` は計画の先頭に入れ子であることを示し、秘密の値が標準出力にも標準エラーにも出ず、
通常時は `hide` の空ファイルの内容（空）と seccomp のフィルタをファイル記述子で用意し、記号を番号に
置き換え、組み立てた環境で `bwrap` を exec する。exec 先の観測はステップ 15。
Shown by: test — `a_policy_diagnostic_exits_125_with_one_stderr_line`、
`print_plan_with_a_diagnostic_prints_no_plan`、`print_plan_exits_zero_and_prints_the_resolved_command`、
`a_nested_launch_warns_and_runs_the_command_without_bwrap`、
`a_nested_launch_leaves_the_environment_unchanged`、`a_nested_launch_resolves_the_command_on_the_host_path_and_exits_127_when_missing`、
`a_control_character_in_a_warning_is_escaped`、
`a_nested_print_plan_reads_the_policy_and_marks_the_plan_as_nested`、
`secret_values_never_reach_stdout_or_stderr`（ビルド済みバイナリを起動）。
Left to the implementer: パイプかメモリ上のファイルか、番号の割り当て、計画の整形。
Stop and hand back if: `CommandExt::exec` と `pre_exec` での `dup2` の組み合わせで記述子が
`bwrap` に渡らない（`CLOEXEC` が残る）ことが分かり、`libc` の直接呼び出しで解決できない。

## Step 12 — 決定性の観測

Purpose: 仕様#1 の「同じ入力から同じ計画」を、ビルド済みバイナリの `--print-plan` を 2 回走らせて
観測する。
Specification: 仕様#1 結果、仕様#6.4 順序。
Prerequisites: ステップ 11。
May change: `tests/cli.rs`。
Done when: 同じポリシー・同じ一時ワークスペース・同じ環境で `--print-plan` を 2 回実行した
標準出力が一致する。
Shown by: test — `print_plan_is_identical_across_two_runs`。
Left to the implementer: なし。
Stop and hand back if: 一致しない。原因を報告し、走査の順序ならステップ 6 へ、環境ならステップ 8 へ
差し戻す。このステップでは製品コードを直さない。

## Step 13 — 同梱プロファイル

Purpose: 仕様#16 が定める `examples/profile/default.toml` を書き、読み込みと置き場の検査を通す。
Specification: 仕様#16 公開ドキュメント（同梱プロファイルの箇条書き）。
Prerequisites: ステップ 11。現行 jail のマウント表 `/home/mizumi/develop/dotfiles/ai/codex/jail.conf`
を読めること。
May change: `examples/profile/default.toml`、`tests/` の該当ファイル。
Done when: `default.toml` が仕様#16 の箇条書きの内容（現行 jail のマウント表の写し + 4 つの `rw` +
`/tmp` と `/run/user` の `hide` + 走査 + `hide-mounts` + `env.unset` の一群 + `${config_dir}/secrets/`
の秘密）を含み、一時ディレクトリのワークスペースに対してステップ 3〜7 の読み込み・合成・置き場の
検査を診断なしで通る。
Shown by: test — `the_bundled_default_profile_loads_and_passes_the_placement_checks`。
Left to the implementer: 項目の並びとコメント。
Stop and hand back if: 現行 jail のマウント表に、本仕様の 4 指令で表せない行がある。

## Step 14 — README と CHANGELOG

Purpose: 仕様#16 が定める README（英語）と、0.1.0 の CHANGELOG の項目を書く。
Specification: 仕様#16 公開ドキュメント、仕様#17 版とリリース、仕様#18 0.1 で作らないもの。
Prerequisites: ステップ 13（README のコマンド例とプロファイルが実物と合うように）。
May change: `README.md`、`CHANGELOG.md`。
Done when: README に目的・インストール・ポリシーの書き方・CLI・既知の隙間 14 件・第 5.6 節の
理由と dotfiles の書き方・秘密の置き場・`/tmp/process-wrap` の作成（利用者かシムが作る。無ければ
その項目が飛ばされて `/tmp` が空のままになる）・`/tmp` を `hide` にする理由と `/tmp/process-wrap` が
共有場所であること・`examples/profile/default.toml` への参照が載り、`CHANGELOG.md` に 0.1.0 の
項目がある。
Shown by: check — 仕様#16 の箇条書きと既知の隙間 14 件を 1 項目ずつ README の見出しまたは段落と
突き合わせ、対応の無い項目が 0 であることをレビューで確認する。`rg -n '0.1.0' CHANGELOG.md`（1 行以上）。
Left to the implementer: README の構成と英語の文言。
Stop and hand back if: なし。

## Step 15 — ビルド済みバイナリで仕様#13 の表と隔離の中を観測する

Purpose: 実際の bwrap で、終了コードの表の全行、引数の透過、順序の 5 場面、指令の意味、ネットワーク
`none`、秘密、git の書き換え、seccomp、入れ子と並列、位置の拒否、ファイルを書かないことを観測する。
Specification: 仕様#15.2 ビルド済みバイナリのテスト、仕様#1 結果（引数を書き換えない）、仕様#13
出力と終了コードの契約、仕様#6.1 指令の意味（`rw-file` の rename、ソケットへの接続）、仕様#6.4
順序、仕様#6.5 作業場所への警告と危険な広さの拒否（観測条件）、仕様#7 ネットワーク、仕様#9
認証情報、仕様#10 git の URL 書き換え（観測条件）、仕様#11 端末の保護、仕様#12.1 入れ子、仕様#12.2
並列、仕様#14 実行時の境界（永続状態なし）、仕様#4.2 コマンドの解決（隠したディレクトリの中の
コマンド）。
Prerequisites: ステップ 11。開発機に `bwrap` 0.9.0 以上。
May change: `tests/` の統合テストファイルと `tests/common/mod.rs`。
Done when: 次が一時ディレクトリ上のポリシーとワークスペースで観測される。隣り合う段階の診断が同時に成り立つ入力（仕様#13 の検査順序）で先の段階の種類が出る。FIFO と 1 MiB 超のポリシーファイルが仕様#14 のとおり診断で終わる。コマンドの終了コード n が
n で、シグナル s が 128 + s で返り、コマンドの引数が隔離の中で書き換えられずに届く。`hide` の中に
あるコマンドを `/` 付きで指すと bwrap の exec 失敗となり、bwrap の出力と終了コードがそのまま返る
（仕様#13 の「bwrap 自身の失敗」の行の生成手段）。5 場面を隔離の中から読み書きして表どおり。
`rw-file` の対象へ一時ファイルからの rename が失敗し、その場の書き込みは通る。読み取り専用で見える
範囲にあるホストの UNIX ソケットに隔離の中から接続できる。`none` で外部アドレスへの接続が経路なしで
失敗。秘密の変数が隔離の中で読めてファイルは空。隔離の中の `git config --list` に `instead-of` の
組が現れ、ホスト側の組も残る。`TIOCSTI` が `EPERM`（上位ビット付きでも）。x32 ビット付きの
システムコールでプロセスが終了。隔離の中からの `process-wrap` 起動で警告 1 行と外側の境界での実行。
入れ子でない 2 つの起動を同時に行っても両方が終了コードを透過する。ホームをワークツリーとする起動と
`hide` の中を作業ディレクトリとする起動が 125 で終わる。起動の前後で一時ディレクトリの木（ポリシーと
ワークスペース以外）の一覧が変わらない。
Shown by: test — `adjacent_stages_yield_the_earlier_diagnostic`（段階の対ごとに 1 入力）、`a_fifo_policy_file_ends_in_a_diagnostic_from_the_binary`、`a_command_exit_code_passes_through`、`a_command_signal_passes_through_as_128_plus_s`、
`command_arguments_arrive_unchanged`、
`a_command_inside_a_hidden_directory_fails_at_exec_with_bwrap_status`、
`a_hidden_ancestor_does_not_hide_the_rw_worktree`、`an_ro_file_inside_an_rw_directory_is_read_only`、
`a_scanned_env_file_reads_empty`、`the_shared_tmp_subdirectory_is_visible_inside_an_empty_tmp`、
`a_scanned_file_under_an_rw_cache_reads_empty`、`rename_onto_an_rw_file_fails_but_in_place_writes_work`、
`a_host_socket_visible_read_only_is_connectable`、`network_none_has_no_route`、
`a_secret_is_readable_as_a_variable_and_the_file_is_empty`、
`instead_of_appears_in_git_config_inside_with_host_entries_kept`、`tiocsti_is_denied_with_eperm`、
`tiocsti_with_high_bits_is_denied_with_eperm`、`an_x32_syscall_kills_the_process`、
`a_nested_launch_runs_under_the_outer_boundary`、`two_concurrent_launches_both_pass_their_status_through`、
`a_worktree_at_home_exits_125`、`a_cwd_inside_a_hide_exits_125`、
`a_launch_leaves_the_host_tree_unchanged`。
Left to the implementer: 隔離の中で ioctl や x32 のシステムコールを発行する手段（Python が
あれば `fcntl.ioctl` と `ctypes` の `syscall`、無ければ小さな Rust のテスト用バイナリ）。
Stop and hand back if: `bwrap` が無い。実測が仕様#13 の表と食い違う（原因を報告し、該当ステップに
差し戻す。このステップでは製品コードを直さない）。

## Step 16 — 移行の確認（人が行う）

Purpose: 現行の codex 用 jail が bwrap に渡す引数列と、同梱プロファイルでの `--print-plan` の出力を
並べ、マウントの集合が同じであることを確認する。
Specification: 仕様#15.3 移行の確認。
Prerequisites: ステップ 13、15。
May change: なし。dotfiles 側のファイルも変更しない。
Done when: 利用者が、実機で両者を並べて「同じ」と判断した。
Shown by: external — 利用者が行う。`bwrap` という名前で引数をそのまま表示するだけのスクリプトを
一時ディレクトリに置き、その一時ディレクトリを `PATH` の先頭にして
`/home/mizumi/develop/dotfiles/ai/codex/bin/codex` を実行すると、現行 jail の引数列が得られる。同じ
ワークスペースで `process-wrap --profile default --print-plan -- codex` を実行して比べる。合格は
「rw / ro / hide の対象の集合が同じ」。ただし仕様#16 公開ドキュメント が同梱プロファイルに加えると
定めた分（`/tmp` の `hide` と `/tmp/process-wrap`、`/run/user`）は仕様が意図した差なので、集合の
比較から除いて見る。これは人が行う確認で、テストとして残さない。
Left to the implementer: なし。
Stop and hand back if: 仕様#16 が定めた分以外の差が出た。仕様か同梱プロファイルの誤りとして報告する。
