# process-wrap 0.1 実装計画

## Goal

`docs/spec/process-wrap.md` が定める `process-wrap` 0.1 を Rust で実装し、仕様の第 15 節が求める
2 層のテスト（純粋な関数のテストと、実際の bwrap を使うビルド済みバイナリのテスト）が通る状態で、
README と同梱プロファイルまで揃える。

## Specification

`docs/spec/process-wrap.md`（コミット e0df7b4 で承認済み）。以下、仕様の節は `仕様#<見出し>` と書く。
見出しは仕様の `## `/`### ` 行の文字列で、章番号の後の `.` だけ省く（`仕様#3 対応環境` は
`## 3. 対応環境`）。用語は `CONTEXT.md` に従う。本計画で使う「核」は仕様#15.1 純粋な関数の境界 が言う純粋な関数の
集まり、「外周」はその外に置く事実の収集・bwrap の起動・コマンドラインの解釈を指す。

## Approach and why

**純粋な核と薄い外周に分ける。** 仕様#15.1 純粋な関数の境界 が、計画の算出を「合成前のポリシー各段、
コマンドラインの引数、ホストの環境、収集した事実だけから決まる純粋な関数」と定めている。これを
そのまま構造にする。核は「ポリシー各段 + 引数 + 環境 + 事実 → 計画 または 診断」を返し、ファイル
システムにも `std::process` にも触れない。外周は 3 つ。事実を集める（ファイルシステムを読む）、
計画から bwrap を起動する（ファイル記述子を作り exec する）、コマンドラインを解釈する。事実を集める
外周も外部コマンドを起動しない（仕様#14 実行時の境界）。テストは核に対して合成した事実を与えて
計画を比べ、外周に対してはビルド済みバイナリを起動して仕様#13 の表を観測する。

**事実の収集は 2 段階になる。** ポリシーの中の変数（`${worktree}` など）はファイルシステムから
導出するが、どのパスを調べるべきかはポリシーを展開しないと分からない。そこで核を 2 つの純粋な関数に
分ける。1 つ目は「ワークスペースと git の生の事実 → 変数の値 または 診断」、2 つ目は「変数を展開
した後の候補パスの事実 + 走査結果 + マウント一覧 + 秘密の内容 → 計画」。外周はその間で事実を集める。
生の事実（`.git` の種類、`gitdir:` の内容、`commondir` の内容、指し返しの内容、`config` の内容）を
そのまま渡し、相互リンクの検証は核で行う。検証の規則をテストで固定するためである。

**crate は lib + bin にする。** 純粋な核を統合テストから直接呼ぶため、`src/lib.rs` に核と外周を
置き、`src/main.rs` は引数を渡して終了コードを返すだけにする。crate 名とバイナリ名は `process-wrap`。

**順序は「核を内側から」。** ポリシーの読み込みと合成、変数の導出、マウントの解決、環境の組み立て、
コマンド解決と引数列、の順に核を積み、最後に外周をつないで bwrap で観測する。各段階の出力が次の
段階の入力になるので、この順なら手戻りが無い。

**テストファーストで進める。** 各ステップは、そのステップが固定する振る舞いのテストを先に書いて
失敗させ、通し、整える。テスト名は振る舞いを述べる文にする（内部の関数名やモックの名前を入れない）。
下に挙げるテスト名は振る舞いを指定するもので、識別子の細部は実装者が整えてよい。

## Reuse record（層ごとの採用・自作の記録）

前例として参照する姉妹プロジェクトは `/home/mizumi/develop/run-if-present`（同じ利用者の Rust 製
CLI。`Cargo.toml`、`PROJECT.md`、`tests/common/mod.rs` の形を踏襲する）。

| 層 | 判断 | 理由 |
|---|---|---|
| コマンドライン解釈 | 採用: `clap` 4 系（derive）。版は実装時の最新を `=` で固定 | run-if-present と同じ。`--version` が `Cargo.toml` の版を出す（仕様#17 版とリリース）のに使える |
| TOML の読み込み | 採用: `toml` + `serde`（derive） | 仕様#20 未決と委譲 が委譲済み。未知キーの拒否は serde の `deny_unknown_fields` で足りる |
| ワイルドカード（走査の名前、`env.unset`） | 自作（数十行） | 仕様は `*` と `?` だけ。既存の glob crate は `[...]` も受け付け、仕様より広い |
| 実体のパスへの解決 | 標準ライブラリ `std::fs::canonicalize` | 追加不要 |
| マウント一覧の取得 | 自作（`/proc/self/mountinfo` の 5 列目のマウント先と、` - ` 区切りの直後のファイルシステム種別を読む数行。任意欄が可変長なので区切りで探す） | 必要なのはマウント先と種別だけ。専用 crate は過剰 |
| seccomp フィルタ | 自作（BPF 命令 10 個程度の定数） | 規則が 3 つに固定されていて、仕様#11 端末の保護 がバイト列の検査を観測条件にしている。生成 crate を挟むと検査対象が crate の出力になる |
| ファイル記述子の受け渡し（`dup2`、`pipe`） | 採用: `libc` | 標準ライブラリに `dup2` が無い。`nix` はこの 2 つには重い |
| git のメタデータ（`.git` ファイル、`commondir`、`gitdir`、`config` の `core.worktree`） | 自作（各数行） | 読むのは行単位の固定形式。git 実装 crate は仕様#14 が禁じる外部コマンドと同じくらい重い |
| 環境の組み立て、順序付け | 標準ライブラリ（`BTreeMap`、`Vec` の安定ソート） | 仕様#1 の決定性はバイト順の `BTreeMap` で得られる |
| bwrap の起動 | 標準ライブラリ `std::process::Command` + `CommandExt::exec` | 追加不要 |
| ビルド済みバイナリのテスト補助 | 自作（run-if-present の `tests/common/mod.rs` と同じ形） | 外部のテスト crate を入れない前例に合わせる |
| テスト用の一時ディレクトリ | 自作（`std::env::temp_dir` の下に作り `Drop` で消す数行。run-if-present の `TempDir` と同じ） | `tempfile` crate を入れるほどの機能は要らない |

## Scope of change

新規リポジトリのため、すべて新規作成。

- `Cargo.toml`、`Cargo.lock`
- `src/lib.rs`、`src/main.rs`、`src/` 以下のモジュール（構成は実装者に委ねる。下の「Left to the implementer」）
- `tests/` 以下
- `examples/profile/default.toml`
- `README.md`、`CHANGELOG.md`
- `PROJECT.md`（検証コマンドの置き場。ステップ 1 で作る）

`docs/spec/process-wrap.md` と `CONTEXT.md` は変更しない。仕様の不足を見つけたら止めて戻す。
`lefthook.yml`、`.claude/settings.json`、`.claude/hooks/` も変更しない（整形と lint の自動化。
下の Test command を参照）。

## Step order and prerequisites

1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10 → 11 → 12 → 13 → 14 → 15 → 16。ステップ 10（seccomp）は
3〜9 と独立で、2 の後ならいつでもよい。ステップ 12（決定性）と 15（実機観測）は観測だけを行う
ステップで、不一致が出たら該当する前のステップに差し戻す。それ以外は順に依存する。

## Verification map

| 仕様の節 | 証明するステップ |
|---|---|
| 1 結果 | 12（決定性）、15（引数の透過） |
| 2 用語 | 5（ワークスペース、ワークツリー、設定ディレクトリ）、9（計画） |
| 3 対応環境 | 1 |
| 4.1 文法 | 2 |
| 4.2 コマンドの解決 | 9、15 |
| 5.1 形式 | 3 |
| 5.2 パスの書き方 | 3（書式）、5（変数の導出と相互リンク）、6（展開と値を持たない変数） |
| 5.3 段の合成、5.5 効かない指定の拒否 | 4 |
| 5.4 マウント項目の同一性 | 6 |
| 5.6 ポリシーを書き換えられない置き場 | 7 |
| 6.1 指令の意味、6.2 実体への解決と存在しないパス、6.3 生成される項目、6.4 順序 | 6、15 |
| 6.5 作業場所への警告と危険な広さの拒否 | 5（位置の拒否）、7（警告とカレントディレクトリ）、15 |
| 7 ネットワーク | 9、15 |
| 8 環境変数、9 認証情報、10 git の URL 書き換え | 8、11（秘密の値の非表示）、15 |
| 11 端末の保護 | 10（バイト列）、15（実機） |
| 12.1 入れ子、12.2 並列 | 11、15 |
| 13 出力と終了コードの契約 | 2、11、15 |
| 14 実行時の境界 | 9（固定引数、`bwrap` の所在）、11、15（ファイルを書かない） |
| 15.1 純粋な関数の境界 | 3〜10 の純粋なテスト |
| 15.2 ビルド済みバイナリのテスト | 15 |
| 15.3 移行の確認 | 16（人が行う） |
| 16 公開ドキュメント | 13、14 |
| 17 版とリリース | 1、14（tag はリリース時の操作。Out of scope 参照） |
| 18 0.1 で作らないもの | 2 の未知オプションの拒否と 3 の未知キーの拒否（存在しないことの証明はそれ以上できない） |
| 20 未決と委譲 | 計画全体の Left to the implementer に写した |

## Left to the implementer（計画全体）

- モジュールの切り方と名前。核と外周の境界が保たれ、核がファイルシステムと `std::process` に
  依存せず、事実を集める外周が外部コマンドを起動しないことだけが条件。
- 型の名前、テスト関数の識別子、補助関数の抽出。テスト名は振る舞いを述べること。
- `toml` の版と、`serde` の派生の使い方。`clap` の版（4 系）。
- ファイル記述子の作り方（パイプかメモリ上のファイルか）と番号の割り当て（仕様#20 で委譲済み）。
- 計画の表示の整形（仕様#13 が内容だけを定める）。
- 診断と警告の説明文の文言と並び（仕様#20 が委譲。接頭辞、種類、仕様#5.6 の 3 要素だけが契約）。
- 走査の探索順と並列化（仕様#20 が委譲）。
- `Cargo.toml` の `include`、edition（2021 か 2024）。

## Stop conditions（計画全体）

一般の 4 条件（仕様に無い意味の決定が要る／不可逆・特権・危険な操作／事故の拡大／方針を変えても
進まない）に加えて:

- 仕様の節に書かれていない観測可能な振る舞いを決めないと先に進めないとき。仕様に戻す。
- 開発機に `bwrap` 0.9.0 以上が無いとき。ステップ 15 は実際の bwrap を必要とする。
- 開発機に `git` 2.x が無いとき。ステップ 5 のテストは本物の git worktree とサブモジュールを作る。
- 実測が仕様の記述と食い違うとき（例: bwrap の終了コード、マウントの順序の効き方）。仕様の
  事実の記述を疑い、報告して止める。
- `x86_64` 以外でビルドが通ってしまうとき（ステップ 1 の `compile_error!` が効いていない）。

## Test command

このリポジトリにはまだ検証コマンドの取り決めが無い。run-if-present と同じ 4 本をステップ 1 で
`PROJECT.md` に書き、以降のステップはこれを使う。

```
cargo build --locked
cargo test --all-targets --locked
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
```

ビルド済みバイナリのテストは `bwrap` を必要とし、無ければ失敗する（飛ばさない）。仕様#15.2 が
実際の bwrap での観測を要求しているためである。テストは `XDG_CONFIG_HOME` と `HOME` を一時
ディレクトリに向けて実行し、利用者の実際の設定ディレクトリを読まない。

整形と lint は 2 か所で自動化されている（リポジトリに同梱済み）。`lefthook.yml` の pre-commit が
`cargo fmt --check` と `cargo clippy -D warnings` を走らせ、`.claude/settings.json` の hook が
`.rs` を書いた直後に `cargo fmt` を、エージェントが停止するたびに clippy を走らせて警告があれば
停止を拒む。どちらも `Cargo.toml` が無い間は何もしない。実装者はこれを外さない。

## Out of scope

- codex や opencode のシム（仕様#18）。dotfiles 側で書く。
- CI、配布物、リリースの自動化（仕様#17、#18）。
- `AGENTS.md` と `PROJECT.md` のルーティング表の生成。`PROJECT.md` はステップ 1 で検証コマンドと
  仕様への参照だけを書く。
- 仕様の `Status: Draft` を `Approved` に変えること。仕様の変更は本計画の範囲外で、利用者が判断する。
- 0.1.0 の tag を打つこと。仕様#17 版とリリース が求める tag はリリース時の操作で、実装が受け
  入れられた後に利用者がリリースの手順で行う。

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

## Step 11 — 外周の結線: 診断の出力、計画の表示、入れ子、bwrap の起動

Purpose: 核の出力を実際の振る舞いにする。診断と警告の標準エラー出力と終了コード、`--print-plan` の
表示、入れ子の分岐、ファイル記述子の準備と exec。
Specification: 仕様#13 出力と終了コードの契約、仕様#12.1 入れ子、仕様#14 実行時の境界（ファイル
記述子、exec）、仕様#8 環境変数（環境を bwrap に引き継がせる）。
Prerequisites: ステップ 2〜10。
May change: `src/main.rs`、`src/` の起動モジュール（外周）、`tests/cli.rs`。
Done when: 診断が `process-wrap: <種類>: <説明>` の 1 行で標準エラーに出て 125（`command not found`
は 127）で終わり、警告が `process-wrap: warning: ` で始まり、`--print-plan` が診断の無いとき計画を
標準出力に出して 0 で終わり診断のあるときは計画を出さず、`PROCESS_WRAP=1` のとき `--print-plan`
以外はポリシーを読まず環境を変えず警告 1 行の後にコマンドを exec し、`PROCESS_WRAP=1` での
`--print-plan` は計画の先頭に入れ子であることを示し、秘密の値が標準出力にも標準エラーにも出ず、
通常時は `hide` の空ファイルの内容（空）と seccomp のフィルタをファイル記述子で用意し、記号を番号に
置き換え、組み立てた環境で `bwrap` を exec する。exec 先の観測はステップ 15。
Shown by: test — `a_policy_diagnostic_exits_125_with_one_stderr_line`、
`print_plan_with_a_diagnostic_prints_no_plan`、`print_plan_exits_zero_and_prints_the_resolved_command`、
`a_nested_launch_warns_and_runs_the_command_without_bwrap`、
`a_nested_launch_leaves_the_environment_unchanged`、
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
Done when: README に目的・インストール・ポリシーの書き方・CLI・既知の隙間 12 件・第 5.6 節の
理由と dotfiles の書き方・秘密の置き場・`/tmp/process-wrap` の作成（利用者かシムが作る。無ければ
その項目が飛ばされて `/tmp` が空のままになる）・`/tmp` を `hide` にする理由と `/tmp/process-wrap` が
共有場所であること・`examples/profile/default.toml` への参照が載り、`CHANGELOG.md` に 0.1.0 の
項目がある。
Shown by: check — 仕様#16 の箇条書きと既知の隙間 12 件を 1 項目ずつ README の見出しまたは段落と
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
Done when: 次が一時ディレクトリ上のポリシーとワークスペースで観測される。コマンドの終了コード n が
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
Shown by: test — `a_command_exit_code_passes_through`、`a_command_signal_passes_through_as_128_plus_s`、
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
