# process-wrap 0.1 実装計画 A2: 仕様の改訂に計画 A の成果物を追従させる

この計画は全体像 `docs/plans/implement-0.1.md` の 3 本の間に挟む短い計画である。計画 A（ステップ 1〜5）の
マージ後、計画 B（ステップ 6〜10）の前に回す。方針、再利用の判断、計画全体の「Left to the implementer」
「Stop conditions」「Test command」は全体像に書いてあり、この計画はそれを前提にする。実装者は先に
全体像を読むこと。ステップ番号は全体像の番号と衝突しないよう 5.1〜5.5 とする。

## Goal

コミット 1142262 で仕様に書き足した決定のうち、計画 A が作ったモジュール（コマンドラインの解釈、
ホスト環境、ポリシーの読み込みと合成、変数の導出、事実の収集、診断）に関わるものを、テストが通る
状態で実装に反映する。計画 B が前提にする核の入力（4 変数がすべて実体、ホームの検査済み、読み込みの
防御済み）をここで揃える。

## Specification

`docs/spec/process-wrap.md`（コミット 1142262 で改訂を承認済み）。節の書き方は全体像と同じ
（`仕様#4.1 文法` は `### 4.1 文法`）。用語は `CONTEXT.md` に従う。

## Approach and why

改訂で決まった振る舞いは、計画 A のモジュールに散らばる小さな規則の集まりで、互いに独立している。
関わるモジュールごとに 1 ステップにまとめ、各ステップは既存のテストが通る状態を保ったまま規則を
1 つずつ足す。計画 B に混ぜないのは、計画 B が隔離の安全性の核心（順序、置き場の拒否、秘密、seccomp）で、
そのレビューに無関係な直しを混ぜたくないためである。

改訂の決定のうち、計画 B と C の範囲に属するもの（秘密ファイルの置き場の検査と値の上限、マウントの
解決の段階の中の順序、入れ子の分岐とコマンド解決）はこの計画では扱わない。下の「Out of scope」を参照。

## Scope of change

- `src/cli.rs`、`src/environment.rs`、`src/layers.rs`、`src/variables.rs`、`src/workspace_facts.rs`、
  `src/diagnostic.rs`、`src/main.rs`、`src/lib.rs`。読み込みの共通部品を切り出すなら `src/` に新しい
  モジュールを足してよい。
- `tests/cli.rs`、`tests/layers.rs`、`tests/policy.rs`、`tests/variables.rs`、`tests/common/mod.rs`。
- `Cargo.toml` と `Cargo.lock` は変えない（必要な依存は揃っている）。

全体像の「Scope of change」にある変更しないもの（仕様、用語集、整形と lint の自動化）はここでも
変更しない。

## Step order and prerequisites

5.1 → 5.5 の順が自然だが、5.1〜5.4 は互いに独立で、5.5 だけが 5.1 と 5.2 の後。前提は計画 A が `main` に
マージされていること（コミット 0551cdb 以降）と、Rust 1.85 以上、cargo、`git` 2.x、`mkfifo`。

## Verification map（この計画の分）

| 仕様の節 | ステップ |
|---|---|
| 2 用語（ホームディレクトリ、設定ディレクトリ、ワークスペース） | 5.2 |
| 4.1 文法（`NAME` の制約、4 形式の排他、繰り返し、`=` の形、`-` 始まりと空文字の値） | 5.1 |
| 5.2 パスの書き方（変数はすべて実体、`~` の展開先、`.git` と G の下のファイル） | 5.2 |
| 6.5 作業場所への警告と危険な広さの拒否（カレントディレクトリの取得失敗） | 5.5 |
| 13 出力と終了コードの契約（制御文字の逃がし、`usage`・`policy`・`path`・`env` 行の追加分、検査順序の段階 1〜6） | 5.1、5.2、5.3、5.4、5.5 |
| 14 実行時の境界（ファイルの読み方: ポリシーファイル） | 5.3 |
| 20 未決と委譲（見える表記の形） | 5.4 |

## Left to the implementer / Stop conditions / Test command

全体像のとおり。この計画に固有の停止条件: 計画 A の成果物の型や関数が、このステップが前提とする形と
食い違うとき（例: `HostEnvironment` が `HOME` の検査を持てない構造になっている）。推測で合わせず、報告して
止める。検証コマンドは `PROJECT.md` の 4 本。テストは `HOME` と `XDG_CONFIG_HOME` を一時ディレクトリに
向けて実行する。

## Out of scope

- 仕様#5.6 の秘密ファイルの前置き検査、仕様#9 の値の上限 64 KiB と秘密ファイルの読み方（計画 B の
  ステップ 7、8。ステップ 8 はこの計画のステップ 5.3 の読み込みの部品を使う）。
- 仕様#13 の検査順序の段階 7（マウントの解決の中の順序）と段階 8、9（計画 B のステップ 6〜9）。
- 仕様#12.1 の入れ子の分岐と入れ子でのコマンド解決（計画 C のステップ 11）。
- `~` の展開そのもの（計画 B のステップ 6）。この計画はその展開先であるホームディレクトリの実体を
  用意するところまで。
- 警告と計画の表示への制御文字の逃がしの適用（計画 C のステップ 11。ステップ 5.4 の逃がしの部品を再利用する）。
- ビルド済みバイナリでの仕様#14 の読み方と仕様#13 の順序の観測（計画 C のステップ 15）。

計画 B と C の該当ステップは、この計画と同じコミットで改訂後の仕様に合わせて更新してある。

---

## Step 5.1 — コマンドラインの文法の締め付け

Purpose: 仕様#4.1 に足された制約（`NAME` の形、4 形式の排他、繰り返し不可のオプションの繰り返し、
`--opt=VALUE` の形、`-` 始まりと空文字の値）を、種類 `usage` の診断と終了コード 125 で観測できる
ようにする。
Specification: 仕様#4.1 文法、仕様#13 出力と終了コードの契約（`usage` 行、検査順序の段階 1〜2）。
Prerequisites: なし。
May change: `src/cli.rs`、`tests/cli.rs`。
Done when: `--profile` の名前が空、`/` を含む、`.`、`..` のいずれかなら `usage`。`--help` または
`--version` が他のどの引数（正当なオプション、`--`、`COMMAND` を含む）と一緒でも `usage`。`--rw` と
`--hide` 以外のオプションを 2 回書くと `usage`。値を取るオプションの語を分けた形で `-` 始まりの語が
続くと `usage`（`-` 単独を含む）。空文字の値はどちらの形でも `usage`。`--profile=NAME` などの `=` の
形が語を分けた形と同じ意味で受け付けられる。既存の `help_or_version_beside_an_invalid_command_line_is_a_usage_diagnostic`
はそのまま通る。
Shown by: test — ビルド済みバイナリ: `a_profile_name_that_is_not_a_single_path_component_is_a_usage_diagnostic`
（空、`/` 入り、`.`、`..` の 4 入力を 1 本で回す）、`help_beside_a_valid_option_is_a_usage_diagnostic`
（`--help --profile x`、`--version -- true`、`--print-plan --help`）、
`a_repeated_single_use_option_is_a_usage_diagnostic`（`--profile a --profile b`、`--print-plan --print-plan`。
`--rw` の 2 回は通ることも同じテストで見る）、`a_dash_led_option_value_is_a_usage_diagnostic`
（`--profile --rw`、`--workspace -`。排他や繰り返しに当たらない入力で切り分ける）、`an_empty_option_value_is_a_usage_diagnostic`
（`--workspace ""`、`--profile=`）。lib の解釈関数: `the_equals_form_means_the_same_as_the_separated_form`。
Left to the implementer: clap の設定で実現するか、解釈後に自前で検査するか。`usage` の説明文。
Stop and hand back if: clap で `--opt=VALUE` の形を受け付けつつ語を分けた形の `-` 始まりだけを拒む
設定が作れない（その場合は自前の検査に切り替えてよいが、`=` の形の意味が変わるなら止める）。

## Step 5.2 — ホームディレクトリ、設定ディレクトリ、ワークスペースの健全性と実体化

Purpose: 仕様#2 に新設された「ホームディレクトリ」の検査、`XDG_CONFIG_HOME` の空と相対の扱い、
4 変数がすべて実体であること、ワークスペースがディレクトリであることを、核と外周に入れる。
Specification: 仕様#2 用語（ホームディレクトリ、設定ディレクトリ、ワークスペース）、仕様#5.2 パスの
書き方（変数の表の前後の段落、`.git` と G の下のファイルの段落、末尾のワークスペースの段落）、
仕様#13（`env` 行、`path` 行の「ワークスペースが存在しないかディレクトリでない」「設定ディレクトリの
実体のパスを得られない」）、仕様#14 実行時の境界（成功の観測条件の `G/commondir` のシンボリックリンク）。
Prerequisites: なし。
May change: `src/environment.rs`、`src/workspace_facts.rs`、`src/variables.rs`、`tests/variables.rs`、
`tests/layers.rs`。
Done when: 外周が集めて核に渡す事実に、次が加わる: `HOME` の生の値とその実体化の結果（得られたか、
得られた実体がディレクトリか）、ワークスペースの実体の種別、設定ディレクトリの実体化の結果。核は
その事実から次を判定する: `HOME` が無い・空・絶対パスでない・実体のパスを得られない・実体が
ディレクトリでないとき種類 `env`（ポリシーが `~` を使うかにかかわらず）。`${config_dir}` の値が
設定ディレクトリの実体のパスで、実体を得られなければ `path`。ワークスペースの実体がディレクトリで
なければ `path`。設定ディレクトリの導出では、`XDG_CONFIG_HOME` が空か絶対パスでないときは未設定と
同じく `~/.config/process-wrap/` になる。ホームディレクトリの実体が、計画 B のステップ 6 が `~` の
展開に使える形（核の出力、または核に渡す事実）で得られる。`.git` と G の下のファイルは、開くファイル
自身がシンボリックリンクなら辿らず、経路の途中のリンクは辿る（計画 A の実装がすでにそうなっていれば
変えず、テストで固定する）。
Shown by: test — 核（事実を直接与える純粋なテスト）: `an_empty_home_is_an_env_diagnostic`、
`a_relative_home_is_an_env_diagnostic`、`a_home_without_a_real_path_is_an_env_diagnostic`、
`a_home_that_is_a_regular_file_is_an_env_diagnostic`、`a_config_dir_without_a_real_path_is_a_path_diagnostic`、
`a_workspace_that_is_a_regular_file_is_a_path_diagnostic`。外周（一時ディレクトリを作り、収集関数 → 核の
順で呼ぶ。`raw_git_facts_are_read_from_a_real_linked_worktree` と同じ形）:
`the_config_dir_variable_is_the_real_path`（`XDG_CONFIG_HOME` をシンボリックリンク経由のパスにして、
値がリンク解決後であることを見る）、`a_symlinked_commondir_is_a_path_diagnostic`（`G/commondir` を
通常ファイルへのシンボリックリンクにする）、`a_gitdir_reached_through_a_symlinked_directory_is_accepted`
（G 自身をリンク経由のパスで指す）。設定ディレクトリの導出（lib。`HostEnvironment` の導出関数と
`load_layers` で書く）: `an_empty_xdg_config_home_falls_back_to_the_home_config_dir`、
`a_relative_xdg_config_home_falls_back_to_the_home_config_dir`。この 2 本の観測は、一時の
`~/.config/process-wrap/profile/default.toml` に壊れた TOML を置いて `policy` になること（そこが読まれた
証拠）と、相対の `XDG_CONFIG_HOME` の下に正しいプロファイルを置いても結果が変わらないこと（そこは
読まれない証拠）で行う。既存の `a_missing_home_is_an_env_diagnostic` と
`the_config_dir_follows_xdg_config_home` は仕様の新しい文に合わせて断定と呼び方を直してよい。
Left to the implementer: 事実の構造体の形。`HOME` の検査をどのモジュールに置くか（外周で実体化し、
核で判定する分担は保つ）。ただし判定は、ワークスペースの事実を必要とせず、ポリシーの読み込みより前に
単独で呼べる関数にする（ステップ 5.5 が段階 4 として使う。設定ディレクトリの導出が `HOME` に依存する
ため）。
Stop and hand back if: なし。

## Step 5.3 — ポリシーファイルの読み方の防御

Purpose: プロファイルと `--policy-file` の読み込みに、仕様#14 のファイルの読み方（通常ファイルに限る、
待たずに開く、1 MiB の上限、シンボリックリンクは辿る）を入れる。
Specification: 仕様#14 実行時の境界（「ファイルの読み方」の段落）、仕様#13（`policy` 行の「通常ファイルで
ないか第 14 節の上限を超える」）。
Prerequisites: なし。
May change: `src/layers.rs`、`src/workspace_facts.rs`（`.git` 関連の読み方と部品を共有する場合）、
共通部品を置く新しいモジュール、`tests/layers.rs`。
Done when: `--policy-file` またはプロファイルが FIFO・ディレクトリ・その他の通常ファイル以外を
指すと、待たずに種類 `policy` の診断になる。1 MiB を超えるポリシーファイルは `policy`。シンボリック
リンクの先の通常ファイルは読める。`.git` 関連の読み方（開くファイル自身のリンクを辿らない）は
変わらない。
Shown by: test — `a_fifo_policy_file_is_a_policy_diagnostic`（`mkfifo` で作った FIFO を `--policy-file`
に指す。テストが止まらず返ること自体が観測）、`an_oversized_policy_file_is_a_policy_diagnostic`
（内容は正しい TOML（コメント行の繰り返しなど）。1 MiB + 1 バイトが `policy` になり、同じ内容のちょうど
1 MiB が読めることを 1 本で対にする。解析失敗も `policy` なので、内容が正しくないと上限の証拠に
ならない）、`a_policy_file_behind_a_symlink_is_read`。
Left to the implementer: `.git` 関連の読み込み関数と部品を共有するか（辿る・辿らないの違いを引数に
するか、別関数にするか）。ただし部品は診断の種類を呼び出し側が決められる形にし、計画 B のステップ 8 が秘密ファイル（`secret`）に同じ部品を使えるようにする。
Stop and hand back if: なし。

## Step 5.4 — 診断に埋め込む値の制御文字の逃がし

Purpose: 診断の説明に埋め込む値に含まれる 0x00〜0x1F と 0x7F を見える表記に逃がし、診断が 1 行で
制御文字を含まないようにする。
Specification: 仕様#13 出力と終了コードの契約（制御文字の段落）、仕様#20 未決と委譲（見える表記の
形と不正な UTF-8 の表し方は委譲）。
Prerequisites: なし。
May change: `src/diagnostic.rs`、`tests/policy.rs`。
Done when: ポリシーファイルのキー名や変数名、`--profile` の値に ESC や BS などの制御文字を含めても、
診断の文字列に 0x00〜0x1F（末尾の改行を除く）と 0x7F が含まれない。既存の改行の逃がし
（`a_newline_in_a_variable_name_keeps_the_diagnostic_on_one_line`）はそのまま通る。
Shown by: test — `a_control_character_in_a_variable_name_is_escaped_in_the_diagnostic`（TOML の
`\u001b` エスケープで ESC を変数名に入れる。変数名を `x\u001bMARKER` のようにして、診断に `MARKER` が
残ること（値が埋め込まれている証拠）と、バイト列に 0x1B が無いことの両方を見る。表記の形は
固定しない）。
Left to the implementer: 見える表記の形、不正な UTF-8 の表し方（仕様#20 の委譲）。逃がしの関数は `Diagnostic` の外（計画 C の警告と計画の表示）からも呼べる形にする。
Stop and hand back if: なし。

## Step 5.5 — 検査順序の段階 1〜6 とカレントディレクトリの取得

Purpose: 仕様#13 の検査順序のうち、この計画の範囲にある段階（`--help`/`--version` の形 → 文法 →
カレントディレクトリ → ホームディレクトリ → ポリシーの読み込みと合成 → ワークスペースと変数の導出）を
その順で実行する入口を作り、カレントディレクトリの取得失敗を `Diagnostic` 型を通した `path` の診断に
する。
Specification: 仕様#13 出力と終了コードの契約（検査順序の段階 1〜6、`path` 行の「カレントディレクトリが
取得できない」）、仕様#6.5 作業場所への警告と危険な広さの拒否（カレントディレクトリの取得失敗の
段落）。
Prerequisites: ステップ 5.1、5.2。
May change: `src/main.rs`、`src/lib.rs`、`src/cli.rs`、段階を並べる新しいモジュール、`tests/cli.rs`。
Done when: コマンドラインの解釈は、カレントディレクトリ無しで文法を判定できる（相対パスの絶対化は
段階 3 の後に行うか、取得失敗を段階 2 の後まで保留する。現在の `interpret` はカレントディレクトリを
引数に取るので、この形は変わってよく、既存の lib テストの呼び方も変わってよい）。`--help` と
`--version` はカレントディレクトリが取れなくても 0 で終わる。文法の誤りはカレントディレクトリが
取れなくても `usage`。カレントディレクトリが取れず文法が正しいときは
`Diagnostic` 型を通した `path`。ホームディレクトリの検査はポリシーの読み込みより前で、`HOME` が
条件に反しプロファイルも壊れているときは `env`。ポリシーの読み込みは変数の導出より前で、
プロファイルが壊れワークスペースも無いときは `policy`。段階 6 まで通った起動は、計画 C のステップ 11 が bwrap に
つなぐまで、何も出力せず 0 で終わる（計画 A の暫定のまま。仕様に無い暫定で、テストで固定しない）。
Shown by: test — ビルド済みバイナリ: `help_from_a_deleted_current_directory_exits_zero`、
`an_unknown_option_from_a_deleted_current_directory_is_a_usage_diagnostic`、
`a_deleted_current_directory_is_a_path_diagnostic`。この 3 本は、子プロセスの中で一時ディレクトリに
移ってからそれを消し、その後に exec する補助を `tests/common/mod.rs` に足して使う（`CommandExt::pre_exec`
で `set_current_dir` と `remove_dir` を行うか、`sh -c 'cd "$1" && rmdir "$1" && exec "$0" "$@"'` を挟む。
`Command::current_dir` は消したディレクトリでは spawn 自体が失敗し、テストプロセス自身の作業ディレクトリを
変えると並列の他のテストと競合するので、どちらも使わない）、`a_bad_home_beside_a_broken_profile_is_an_env_diagnostic`、
`a_broken_profile_beside_a_missing_workspace_is_a_policy_diagnostic`。
Left to the implementer: 段階を並べる関数の置き場と形。段階 6 までの結果を保持する型。ただし段階 2 と
3 の間に、計画 C が入れ子の分岐（仕様#12.1）を差し込める形にしておく。段階 6 まで通った起動が 0 で
終わる暫定の振る舞いはテストで固定しない。
Stop and hand back if: 段階 3〜6 を通すために、計画 B の範囲の処理（展開、マウントの解決）を先取り
しないと成り立たない構造になった。
