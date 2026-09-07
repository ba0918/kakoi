# process-wrap 0.1 実装計画 E: 入れて即使える（仕様改訂 37fd24b）の実装

この計画は、計画 A〜D で実装した 0.1 に、仕様の改訂（コミット 37fd24b、`docs/spec/process-wrap.md`）を
反映する。全体像 `docs/plans/implement-0.1.md` の「Approach and why」「Left to the implementer」
「Stop conditions」「Test command」はそのまま前提にする。全体像の Verification map は改訂前の仕様に
対するもので、改訂で変わった節の割り当ては本計画の表が優先する。実装者は先に全体像と、仕様の 2 節
（組み込みの既定、シム、セットアップスキル）・4.1 節（`init` を含む）・5.2 節・5.3 節・5.6 節の末尾・
12.1 節・12.2 節・13 節・14 節・15.1 節・15.2 節・16 節・18 節・20 節を読むこと。ステップ番号は全体像から続けて 26〜32。

## Goal

`cargo install --path .` の直後に `process-wrap -- COMMAND` が組み込みの既定で起動し、
`process-wrap init [NAME]` が既定を手元のファイルに書き出し、codex 用シムの雛形とセットアップスキルが
README から辿れる `process-wrap` 0.1。

## Specification

`docs/spec/process-wrap.md`（コミット 37fd24b で改訂。承認済み）。節の書き方は全体像と同じ。用語は
`CONTEXT.md` に従う（「組み込みの既定」「シム」「セットアップスキル」が加わった）。改訂で「変わった
規則」の一覧は本計画の Verification map に写してある。

## Approach and why

純粋な関数の境界（`mounts::Candidates`／`MountFacts`、`src/startup.rs` が両者を渡す形）を保ったまま、
3 つの変更を入れる。

- **設定ディレクトリの存在を事実にする。** いまは `derive_variables` が `RealEntry::Missing`（名前が
  無い場合とリンク切れをまとめた値）のときだけ `path` で止まり、通常ファイルの設定ディレクトリは
  `NotDirectory` として通ってしまう。改訂後は、無ければ `${config_dir}` が値を持たない変数になり、5.6 節の
  保護対象からも外れ、壊れていれば（リンク切れ、通常ファイル）`path`。パスの成分を根から順に見て、最初に
  失敗した成分が「名前が無い」か「壊れている（リンク切れ、通常ファイル、その他の失敗）」かを返す小さな
  探査を外周に 1 つ作り、純粋な側には結果だけを渡す。同じ探査（成分の分類）を、5.3 節の
  「`<設定ディレクトリ>/profile/default.toml` が無い」の判定と 4.1 節の `init` の前置きの検査にも使う。
  共有するのは成分の分類であって、判定するパスは 3 か所で違う（5.2 節は設定ディレクトリ自身、5.3 節は
  `default.toml` までの全成分、`init` は書き出す先までの全成分）。設定ディレクトリはあるが `profile/` が
  無い起動では `default.toml` は「無い」が `${config_dir}` は値を持つ。5.3 節が「名前が無い」と分類しない
  失敗（権限エラーなど）は「壊れている」側で、`policy` になる（「無い」の定義が「名前が存在しない」
  だけだから）。
  `Variables.config_dir` を値を持たない形にすると、純粋な側の次の箇所に分岐が要る。どれも仕様に根拠が
  ある: `mounts::expand` の `${config_dir}` の展開（5.2 節、値を持たない変数）、`candidates` の
  `secrets/` の候補と `secret_items` の `secrets/` の `hide`（6.3 節「存在すれば」）、`protected_paths`／
  `check_protected_paths`（5.6 節、存在しない設定ディレクトリは検査しない）、`plan_text` の変数の表示
  （5.2 節の観測条件、`${git_common_dir}` の先例）。この一覧に無い分岐が要ると分かったら止める。
- **組み込みの既定は段の出どころの 1 つ。** `LayerOrigin` に「組み込み」を足し、`load_layers` が
  `--profile` が `default` で `profile/default.toml` が「無い」ときだけ、ビルド時に埋め込んだ
  `examples/profile/default.toml`（`include_str!`。`Cargo.toml` の `include` は `/examples/**` を含む）を
  グローバルスコープとして読む。段は増えない。計画の「使ったポリシーファイル」の欄には、パスの代わりに
  文字列 `process-wrap init` を含む語を出す（この文字列だけが契約）。
- **`init` は外周の別経路。** `cli` が `--` の前の語 `init` を見て `Parsed::Init(NAME)` を返し
  （clap の `Arguments` は `command` を `last = true` で取るので、生の語列を見る層で判定する。
  `check_option_values` と同じ場所）、`startup` はホームディレクトリの検査だけを通して `Outcome::Init` を
  返し、`main` が新しい外周モジュールで書き込みを行う。製品の中でファイルを書くのはこの経路だけで、
  純粋な側と `startup::prepare` の残りは触らない。

診断の文言・計画の表示の整形・fixture の配置はテストで固定しない（契約は 13 節の種類と終了コード、
計画の欄の文字列 `process-wrap init`、4.1 節が `init` に要求する標準出力の 1 行と `secrets/` のモード、
`init` の `path` の診断の説明が対象のパスを含むこと、`init` の出力と `examples/profile/default.toml` の
バイト一致）。

## Scope of change

- `src/cli.rs`、`src/startup.rs`、`src/main.rs`、`src/variables.rs`、`src/workspace_facts.rs`、
  `src/regular_file.rs`、`src/layers.rs`、`src/mounts.rs`、`src/placement.rs`、`src/plan.rs`、
  `src/plan_text.rs`、`src/diagnostic.rs`、`src/lib.rs`、新規の外周モジュール 1 つ（`init` の書き込み）
- `tests/cli.rs`、`tests/plan.rs`、`tests/layers.rs`、`tests/variables.rs`、`tests/placement.rs`、
  `tests/mounts.rs`、`tests/bundled_profile.rs`、`tests/common/mod.rs`、`tests/common/fixture.rs`
- `examples/profile/default.toml`、`examples/shim/codex`（新規）、`skills/process-wrap-setup/SKILL.md`
  （新規）、`README.md`、`CHANGELOG.md`

変更しないもの: `docs/spec/`、`CONTEXT.md`、`docs/plans/`、`lefthook.yml`、`.claude/`、`Cargo.toml`
（`include` は既に `/examples/**` を含む。`skills/` と `examples/shim/` はバイナリが読まないので
パッケージに含めなくてよい）、`PROJECT.md`。

## Step order and prerequisites

26 → 27 → 28 → 29 → 30 → 31 → 32。26〜28 はコード（純粋な側とビルド済みバイナリ）、29 は同梱
プロファイル、30〜31 は新しい同梱物、32 は文書。前提は main が 37fd24b 以降であること、開発機に
`bwrap` 0.9.0 以上、テストを root 以外で実行すること。

## Verification map（この計画の分）

| 仕様の節（改訂で変わった規則） | ステップ |
|---|---|
| 5.2 パスの書き方（設定ディレクトリが無ければ `${config_dir}` は値を持たない。存在するのに実体を得られなければ `path`）、5.6 ポリシーを書き換えられない置き場（存在しない設定ディレクトリは検査しない）、13（`path` の行の「存在するのに」） | 26 |
| 2 組み込みの既定、5.3 段の合成（組み込みの既定、「無い」の定義）、13 出力と終了コードの契約（計画の欄の文字列）、15.2 ビルド済みバイナリのテスト（組み込みの既定の 4 場面） | 27 |
| 4.1 文法と `init`、13（`usage`・`path` の行、状況の表の `init` の行、段階 2）、14 実行時の境界（書くのは `init` だけ、書く先）、12.1 入れ子・12.2 並列（`init` の除外）、15.1（`init` の文法）、15.2（`init` の場面）、18（`--force` を作らない） | 28 |
| 16 公開ドキュメント（同梱プロファイルの `secrets` はコメントで例示） | 29 |
| 2 シム、16（`examples/shim/codex`）、18（claude と opencode のシムは作らない） | 30 |
| 2 セットアップスキル、16（`skills/process-wrap-setup/SKILL.md`）、18（インストール手段は作らない） | 31 |
| 16（README のインストールの節、「Not in 0.1」、5.6 節の説明）、17 版とリリース（CHANGELOG） | 32 |

改訂で変わった規則の帰結だが、コードが既に仕様どおりで変更の要らない見込みのもの（ステップ 26 の
レビューで確認だけする。違えば同じステップで直す）: 5.6 節の「値を持たない変数を含む `secrets` の値と
`path-prepend` の項目は検査の対象にならない」は、`protected_paths` が値を持たない項目を既に除いている
（`${git_common_dir}` の先例）。

## Left to the implementer（この計画の分）

- 設定ディレクトリの 3 状態（無い・壊れている・ある）を返す探査を `workspace_facts` と `regular_file` の
  どちらに置くか、純粋な側に渡す型（`Variables.config_dir` を `Option` にするか、別の enum か）。
- `LayerOrigin` の組み込みの表し方と、`Plan.policy_files` に組み込みを載せる型（`PathBuf` の列を
  enum の列にするか、別の欄か）。計画の欄の文言（`process-wrap init` を含めば何でもよい）。
- `init` を認識する層（`check_option_values` の前後）と、`Parsed`／`Outcome` に足す値の名前。書き込みを
  行う外周モジュールの名前と、`profile/` と `secrets/` を作る順、途中で失敗したときに作った
  ディレクトリを残すか消すか（20 節が委譲）。
- 診断の説明の文言と、要素の並び。
- テスト補助の広げ方（`binary(home)` は `XDG_CONFIG_HOME=home/.config` を向けるので、設定ディレクトリの
  無い状態は `home/.config/process-wrap` を作らないだけで作れる）。

## Stop conditions（この計画に固有）

- `include_str!` で `examples/profile/default.toml` を埋め込めない（パスの解決に失敗する）。
- `Variables.config_dir` を値を持たない形にすると、Approach に列挙した箇所以外にも分岐が要ると
  分かった（仕様に無い分岐を足すことになる）。

## Out of scope

同梱プロファイルの `ro ~/.codex/skills` の見直し（20 節の未決）、仕様の Status の変更、0.1.0 の tag、
dotfiles 側のシムの置き換え（雛形を使った利用者側の作業）、セットアップスキルを各 CLI に入れる手順の
自動化。

---

## Step 26 — 設定ディレクトリの存在を事実にし、無ければ `${config_dir}` を値を持たない変数にする

Purpose: 設定ディレクトリが存在しない起動で `path` に止まらず、`${config_dir}` を値を持たない変数として
扱い、5.6 節の保護対象の検査から設定ディレクトリを外す。存在するのに実体を得られない（リンク切れ、
通常ファイル）ときは `path`（通常ファイルは今は通ってしまう。新しい振る舞い）。
Specification: 仕様#5.2 パスの書き方（設定ディレクトリの段落と変数の表、観測条件）、仕様#5.6 ポリシーを
書き換えられない置き場（保護対象の段落末尾の 2 文）、仕様#13 出力と終了コードの契約（`path` の行の
「存在するのに」）、仕様#15.1 純粋な関数の境界（収集した事実）。
Prerequisites: main が 37fd24b 以降。
May change: `src/workspace_facts.rs`、`src/regular_file.rs`、`src/variables.rs`、`src/startup.rs`、
`src/mounts.rs`、`src/placement.rs`、`src/plan.rs`、`src/plan_text.rs`、`tests/variables.rs`、
`tests/plan.rs`、`tests/placement.rs`、`tests/mounts.rs`、`tests/common/fixture.rs`。
Done when（すべて純粋な関数の水準。合成した事実を直接与える。ビルド済みバイナリではポリシーの読み込み
（段階 5）が先に `policy` で止まるので、その観測はステップ 27）: 設定ディレクトリが存在しない事実を
与えた起動で、`${config_dir}` を参照する `ro` 項目が「値を持たない変数」として飛ばされ理由付きで計画に
出て、保護対象の検査が設定ディレクトリを見ず（設定ディレクトリを `rw` の中に置く形が止まらない）、
`secrets/` の `hide` が生成されない。設定ディレクトリがリンク切れ、または通常ファイルの事実を与えた
`derive_variables` は `path` で止まる。既存の `a_config_dir_without_a_real_path_is_a_path_diagnostic`
（`tests/variables.rs`。`Missing` → `path` を固定している）は、改訂後は「壊れている」の例に書き換える
（「無い」の例は新しい規則で通る側になる）。`variables()` 系の fixture は設定ディレクトリがある形のまま
通る。計画の表示で `${config_dir}` が値を持たないことは、`${git_common_dir}` と同じ表示になればよく、
文言は固定しない（レビューで確認）。
Shown by: test — `a_missing_configuration_directory_leaves_config_dir_valueless`（`tests/plan.rs`。
飛ばしの表示、保護対象の除外、`secrets/` の `hide` が無いことの 3 つの観測を 1 つの配置で見る）、
`a_broken_configuration_directory_is_a_path_diagnostic`（`tests/variables.rs`。リンク切れと通常ファイルの
2 例。既存テストの書き換えを含む）。
Left to the implementer: 成分の分類を返す探査の置き場（`workspace_facts` か `regular_file`）と型、
`Variables.config_dir` の形（`Option` か別の enum）。
Stop and hand back if: Approach に列挙した箇所以外にも分岐が要ると分かった（計画全体の Stop conditions）。

## Step 27 — 組み込みの既定

Purpose: `--profile` が `default`（省略時を含む）で `profile/default.toml` が「無い」ときだけ、埋め込んだ
`examples/profile/default.toml` をグローバルスコープにする。「無い」は組み立てたパスの成分を根から順に
見て最初の失敗が「名前が無い」のとき。リンク切れ、通常ファイル、読めないファイルは `policy`。名前を
指定したプロファイルは落ちない。計画の「使ったポリシーファイル」の欄に文字列 `process-wrap init` を
含む。
Specification: 仕様#2 組み込みの既定、仕様#5.3 段の合成（組み込みの既定の段落と観測条件）、仕様#13
（計画の内容の段落）、仕様#15.1（各段の出どころ）、仕様#15.2 ビルド済みバイナリのテスト（組み込みの
既定の 4 場面）、仕様#20 未決と委譲（欄の文言）。
Prerequisites: ステップ 26（設定ディレクトリの探査を共有する）。
May change: `src/layers.rs`、`src/mounts.rs`、`src/plan.rs`、`src/plan_text.rs`、`src/placement.rs`、
`src/startup.rs`、`tests/layers.rs`、`tests/plan.rs`、`tests/cli.rs`、`tests/bundled_profile.rs`、
`tests/common/mod.rs`。
Done when（ビルド済みバイナリの水準）: 設定ディレクトリが無い一時 HOME で `--print-plan` が終了コード 0
で終わり、計画に文字列 `process-wrap init` を含む。`XDG_CONFIG_HOME` が存在しないディレクトリを指す
起動でも同じ（祖先が無い枝。README に書く場面）。同じ状態で `--profile strict --print-plan` が `policy` で
終わる。同じ状態で `${config_dir}` を参照する `ro` 項目を書いた `--policy-file` を与えると、その項目が
飛ばしとして計画に出て終了コード 0。`profile/default.toml` を置くとそれだけが読まれ計画に
`process-wrap init` が無い。`default.toml` がリンク切れ、設定ディレクトリがリンク切れ、設定ディレクトリの
場所が通常ファイル、`profile/` が通常ファイルのそれぞれで `policy`。組み込みの既定の内容が
`examples/profile/default.toml` と同じバイト列であることはステップ 28 の `init` の一致テストが固定する
（このステップでは要求しない）。既存の `tests/bundled_profile.rs` と
`a_missing_profile_file_is_a_policy_diagnostic`（`--profile missing`）はそのまま通る。
Shown by: test — `the_built_in_default_is_used_when_default_toml_is_absent`（`tests/cli.rs`。設定
ディレクトリ無しと、存在しない `XDG_CONFIG_HOME` の 2 例）、
`a_named_profile_never_falls_back_to_the_built_in_default`、
`a_policy_file_overlays_the_built_in_default`（`${config_dir}` の飛ばしの表示を含む）、
`a_broken_default_toml_or_configuration_directory_is_a_policy_diagnostic`（リンク切れ 2 例と通常ファイル
2 例）、`a_present_default_toml_replaces_the_built_in_default`。
Left to the implementer: `LayerOrigin` の組み込みの表し方、計画の欄の文言、`tests/layers.rs` と
`tests/cli.rs` のどちらに各テストを置くか（実ファイルを使う既存の置き方に合わせる）。
Stop and hand back if: `include_str!` が `examples/profile/default.toml` を解決できない。

## Step 28 — `process-wrap init [NAME]`

Purpose: 5 つ目の排他の形 `init [NAME]` を足す。設定ディレクトリ（祖先を含む）と `profile/`・`secrets/`
（0700）を無ければ作り、`profile/NAME.toml` に組み込みの既定を書き、組み立てたままのパスを標準出力に
1 行出して 0 で終わる。既にあれば（名前が種類を問わず存在すれば）書かずに `path`。前置きのリンクは辿り、
辿った先がディレクトリでなければ `path`。`NAME` の違反や他の引数との併用は `usage`。通る検査は文法 →
ホームディレクトリ → 書き込みだけで、入れ子を見ず、`bwrap` の所在も確かめない。
Specification: 仕様#4.1 文法、仕様#4.1 `init`、仕様#13（`usage` と `path` の行、状況の表の `init` の行、
段階 1〜2 と段階 4）、仕様#14 実行時の境界（書くのは `init` だけ、書く先）、仕様#12.1 入れ子（`init` の
除外）、仕様#12.2 並列、仕様#15.2（`init` の場面）、仕様#18 0.1 で作らないもの（`--force`）。
Prerequisites: ステップ 27（埋め込みの内容と設定ディレクトリの探査を使う）。
May change: `src/cli.rs`、`src/startup.rs`、`src/main.rs`、`src/diagnostic.rs`、`src/lib.rs`、新規の
外周モジュール、`tests/cli.rs`、`tests/common/mod.rs`、`tests/common/fixture.rs`、`tests/layers.rs`。
`startup::prepare` の分岐の位置: `init` は `cli::interpret` の直後、入れ子の判定とカレントディレクトリの
取得（段階 3）より前で分け、ホームディレクトリの検査だけを通す（13 節の段階 2 の文）。
Done when: 設定ディレクトリが無い一時 HOME で `init` すると `profile/default.toml` と `secrets/` ができ、
`default.toml` が `examples/profile/default.toml` とバイト単位で一致し、`secrets/` のモードが 0700、
標準出力が組み立てたパス 1 行、標準エラー無し、終了コード 0。もう一度 `init` すると `path` の 125 で
ファイルが変わらない。`init strict` が `profile/strict.toml` を作る。`init ../x`、`init -- sh`、
`init --print-plan` が `usage`。書き出す先がリンク切れなら `path`。設定ディレクトリを別の場所への
リンクにすると、リンク先にできて標準出力はリンクを含む組み立てたパス。`~/.config` が無い状態
（`XDG_CONFIG_HOME` 未設定）では `~/.config` から作って成功する。`profile/` が通常ファイルなら `path`。
`PROCESS_WRAP=1` の環境でも標準エラーに何も出さず 0 で書く。削除済みのカレントディレクトリから
`init` しても 0 で書く。`HOME` を無効にすると `env`。`bwrap` を `PATH` から外しても成功する。`init` の
前後で一時 HOME の木を比べると、差分は設定ディレクトリの各成分・`profile/`・`profile/NAME.toml`・
`secrets/` だけで、`/tmp/process-wrap` は作られない（既存の `tree_snapshot` 補助が使える）。`path` の
診断（既にある、リンク切れ、通常ファイル）の説明が対象のパスを含む。`init` 以外の形の起動でファイル
システムが変わらないことは既存のテストが既に固定している（全体像のステップ 15）。
Shown by: test — `init_writes_the_built_in_default_and_prints_its_path`（`tests/cli.rs`、ビルド済み
バイナリ。バイト一致・モード・標準出力・終了コード・木の差分と `/tmp/process-wrap` の不在）、
`init_refuses_to_overwrite_an_existing_profile`（説明に対象のパスを含む）、`init_takes_a_profile_name`、
`init_rejects_a_bad_name_or_extra_arguments`（`cli::interpret` に対する純粋なテスト。`init ../x`、
`init -- sh`、`init --print-plan` の 3 例）、`init_refuses_a_broken_link_or_a_regular_file_in_the_way`
（書き出す先のリンク切れ、`profile/` が通常ファイルの 2 例。説明に対象のパスを含む）、
`init_follows_a_linked_configuration_directory_and_prints_the_written_path`、
`init_creates_missing_ancestors_of_the_configuration_directory`、
`init_ignores_nesting_the_current_directory_and_bwrap`（`PROCESS_WRAP=1`、削除済みカレント
ディレクトリ、`bwrap` 無しの 3 例）、`init_without_a_usable_home_is_an_env_diagnostic`。
Left to the implementer: `init` を認識する層、`Parsed`／`Outcome` の値の名前、外周モジュールの名前、
ディレクトリを作る順と途中で失敗したときの後始末（20 節が委譲）、既存の `Invocation` を直接構築する
テスト補助への影響の吸収。
Stop and hand back if: `secrets/` を 0700 で作れない環境（umask や ACL）が開発機で見つかった。

## Step 29 — 同梱プロファイルの `secrets` をコメントで例示し、先頭コメントを `init` 前提にする

Purpose: 組み込みの既定で起動した利用者が、置いていない秘密ファイルの警告を起動のたびに見ないように
する。先頭コメントの「`mkdir` と `cp` で置く」案内を、`init` で書き出す前提の文に変える。
Specification: 仕様#16 公開ドキュメント（同梱プロファイルの箇条の `secrets`）、仕様#9 認証情報（不在の
警告）。
Prerequisites: ステップ 27（組み込みの既定でこのファイルが読まれる。ステップ 28 のバイト一致は同じ
ファイルを両側で読むので、この変更に自動で追従する）。
May change: `examples/profile/default.toml`。
Done when: `[secrets]` の `GH_TOKEN` の行がコメントになり、説明コメントが「コメントを外して
`secrets/gh-token` を置く」と読め、先頭コメントに `cp` の手順が無く `init` の名前がある。設定
ディレクトリが無い一時 HOME で組み込みの既定の `--print-plan` を起動しても標準エラーに警告が出ない。
`tests/bundled_profile.rs` と ステップ 28 の一致テストが通る。
Shown by: test — `the_built_in_default_starts_without_a_warning`（`tests/cli.rs`。標準エラーが空で
あること）。
Left to the implementer: コメントの英文。
Stop and hand back if: なし。

## Step 30 — codex 用シムの雛形 `examples/shim/codex`

Purpose: LLM ごとの差（自前のサンドボックスを切るフラグ、サブコマンドの分類）をシムに閉じ込め、
利用者が写して `PATH` に置ける雛形を同梱する。
Specification: 仕様#2 シム、仕様#16 公開ドキュメント（`examples/shim/codex` の段落）、仕様#18 0.1 で
作らないもの（claude と opencode のシム）。
Prerequisites: なし（雛形は `process-wrap` の 4.1 節の起動の形だけを使い、`init` に依存しない。順序は
文書の都合）。
May change: `examples/shim/codex`（新規、実行可能）。
Done when: 雛形が、サブコマンドを「隔離に入れる」と「素通し」に分類し、前者に
`--dangerously-bypass-approvals-and-sandbox` を差し込み、`--cd`／`-C` を `--workspace` に、`--add-dir` を
`--rw` に写し、`/tmp/process-wrap` が無ければ作ってから `process-wrap` を exec し、入れ子を見ず、
`PROCESS_WRAP_SHIM_OFF=1` で素の codex を exec し、分類の確かめ方（`codex --help` の一覧との突き合わせ）を
コメントで持つ。実体の codex の探し方（自分自身を除く `PATH` 探索。仕様には無い、この計画の
具体化）を持つ。仕様 16 節の「人が確かめる条件」3 つは、利用者が自分の環境で確かめるものとして README に
書く（ステップ 32。これも仕様には無い、この計画の追加）。この計画のテストの対象にはしない（仕様 16 節）。
Shown by: check — 上の Done when の各項目（分類、フラグ、`--cd`／`-C`、`--add-dir`、`/tmp/process-wrap`、
入れ子を見ない、`PROCESS_WRAP_SHIM_OFF`、確かめ方のコメント、実体の探し方）を雛形の該当行と 1 つずつ
突き合わせ、対応の無い項目が 0 であることをレビューで確認する。`bash -n examples/shim/codex` が通る。
`shellcheck` が開発機にあれば `shellcheck examples/shim/codex` も通る。
Left to the implementer: サブコマンドの具体の分類（仕様は固定しない。手元の `codex --help` で確かめて
書く）、bash の中での書き方（雛形は bash で書く。旧 jail と同じ）。
Stop and hand back if: なし。

## Step 31 — セットアップスキル `skills/process-wrap-setup/SKILL.md`

Purpose: 環境固有の設定（プロファイルへの追加、シムの設置先、代替コマンドの `path-prepend`）の提案を
LLM に委譲する同梱スキルを置く。
Specification: 仕様#2 セットアップスキル、仕様#16 公開ドキュメント（`skills/process-wrap-setup/SKILL.md`
の段落）、仕様#18 0.1 で作らないもの（インストール手段）。
Prerequisites: ステップ 30（シムの雛形を指す）。
May change: `skills/process-wrap-setup/SKILL.md`（新規）。
Done when: frontmatter の `name` が `process-wrap-setup`、`description` があり、本文に「すること」5 つ
（入っている CLI の確認とプロファイルへの提案、シムの設置先の提案、代替コマンドの `path-prepend`、
`codex --help` の一覧と雛形の分類の突き合わせと利用者への問い、前後の `--print-plan` の並置）と
「守ること」4 つ（差分を見せて承認を得てから書く、書く先は設定ディレクトリと承認したシムの置き場だけ、
`secrets/` の中を読まず書かず案内にとどめる、仕様と README を編集しない）が指示として書かれ、隔離の
外で走ることが書かれている。Agent Skills の検証ツール `skills-ref validate` が開発機にあれば通る。
無ければそのことを報告し、利用者が後で走らせる（仕様 16 節の検証は人の手順）。
Shown by: check — 上の Done when の「すること」5 つと「守ること」4 つと「隔離の外で走る」を SKILL.md の
該当行と 1 つずつ突き合わせ、対応の無い項目が 0 であることをレビューで確認する。`skills-ref validate
skills/process-wrap-setup` が開発機にあれば通る（無ければ、frontmatter の `name` と `description` の存在を
`rg` で確かめ、検証ツールは利用者が後で走らせると報告する）。
Left to the implementer: スキル本文の英文と構成。
Stop and hand back if: なし。

## Step 32 — README と CHANGELOG

Purpose: 仕様 16 節が README に定めた新しい内容を書く。インストールの節は `cargo install --path .` の
直後に動くことを先に書き、「必要なら」の並びで `init`、既定が WSL2 想定であること、gh-token、シムの
雛形、セットアップスキルと隔離の外で走らせることを書く。入れた直後は `/tmp` が空で `gh` が未認証で
あること。「Not in 0.1」から「no built-in default policy」と「no shims」を落とし、claude と opencode の
シム、`init --force`、スキルのインストール手段を「作らない」に。5.6 節の説明の節に `init` の言及。
インストールの節（設定ディレクトリの説明の近く）に、clone 前の dotfiles を `XDG_CONFIG_HOME` に指して
いる間は組み込みの既定で動くこと（仕様 5.3 節が README に書けと言う注意。Known gaps の番号付きの
一覧は仕様 16 節どおり 15 のまま）。シムの雛形とスキルの人が確かめる条件を、それぞれの参照の近くに
書く。CHANGELOG の 0.1.0 に、組み込みの
既定、`init`、雛形、スキル、同梱プロファイルの `secrets` のコメント化を足す。
Specification: 仕様#16 公開ドキュメント（README のインストールの節、雛形、スキルの各段落と観測条件）、
仕様#18 0.1 で作らないもの、仕様#17 版とリリース。
Prerequisites: ステップ 31。
May change: `README.md`、`CHANGELOG.md`。
Done when: README のインストールの節が 16 節の各文と 1 対 1 で対応し、`process-wrap init`、
`examples/shim/codex`、`skills/process-wrap-setup`、`gh skill install`、`PROCESS_WRAP_SHIM_OFF` を含み、
「Not in 0.1」に `no built-in default policy` が無く、Known gaps が 15 項目のまま、CHANGELOG の 0.1.0 に
`init` の項目がある。
Shown by: check — 仕様 16 節の README への要求（インストールの節、雛形、スキル、隙間 16）を 1 項目ずつ
README の見出しまたは段落と突き合わせ、対応の無い項目が 0 であることをレビューで確認する。
`rg -n 'process-wrap init' README.md`（1 行以上）、`rg -n 'examples/shim/codex' README.md`（1 行以上）、
`rg -n 'gh skill install' README.md`（1 行以上）、`rg -n 'XDG_CONFIG_HOME' README.md`（1 行以上）、
`rg -c 'no built-in default policy' README.md` が 0、Known gaps の節の中の番号付き項目が 15、
`rg -n 'init' CHANGELOG.md`（1 行以上）。
Left to the implementer: 英語の文言と構成。
Stop and hand back if: なし。
