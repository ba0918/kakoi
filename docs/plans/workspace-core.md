# 実装計画: workspace 化と kakoi-core の切り出し

## Goal

`kakoi` の CLI でない部分（ポリシーの型と段の合成、マウントと seccomp の計画、bwrap の Command と
ファイル記述子の組み立て）を、同じリポジトリの中の別クレート `kakoi-core` として、CLI を経由せずに
Rust から使える形にする。利用者から見える振る舞いは一切変えない。

## Specification

`docs/spec/kakoi.md`。本計画は仕様のどの節も改訂しない。内部のモジュール構成は同仕様の
「## 20. 未決と委譲」が実装者に委ねている範囲であり、本計画はその委譲の中で構成を決める。

## Approach and why

背景: ネットワーク制限を別クレート `kakoi-net` として足す構想と、二人目の利用者（別リポジトリの
ワークフローエンジン）が kakoi をライブラリとして組み込む想定がある。どちらも「CLI の引数解釈や
設定ディレクトリの探索を通らずに、ポリシーから bwrap の起動までを組み立てる」入口を必要とする。
その入口を切るのは、クレートの境界を引く workspace 化のときが一番安い（後で切ると同じコードを
二度動かす）。`kakoi-net` 自体は本計画の範囲外（仕様の第 14 節と第 18 節の改訂を伴うため、別途
仕様から決める）。

方針: 先に今のクレートの中で境界を作り、最後にファイルを動かす。今の `src/` は、事実集め（IO）と
純粋な計画の分離はできているが、次の 3 か所で CLI 側の型に依存している。

- (a) `src/layers.rs` が `cli::Invocation` を受け取る（`use crate::cli` の行）。
- (b) `src/launch.rs` が `startup::Prepared` を受け取り、exec まで行う。
- (c) `src/startup.rs` の `prepare` が、引数の解釈から計画までを一続きで持ち、環境変数と
  カレントディレクトリをその中で読む。

これらを順に切ってから workspace 化すると、各段階が既存のテスト一式で合否を出せる小さな差分になる。

境界の規則（本計画で固定する。実装者の選択に委ねない）:

- `kakoi-core` は `clap` に依存しない。引数の解釈は CLI 側。
- `kakoi-core` は `std::env`（環境変数、カレントディレクトリ、引数）を読まない。値として受け取る。
  ファイルシステムの事実集め（マウント一覧、走査、ポリシーファイルと秘密ファイルの読み取り、
  `rw-copy` の複製元）は core に残る。
- `kakoi-core` は標準出力・標準エラーに書かない。診断と警告は値として返す。
- `kakoi-core` は exec しない。bwrap の `Command` と、exec まで生かしておくファイル記述子を返す。
  exec は CLI 側。
- 版は `Cargo.toml` の 1 か所（`[workspace.package]`）だけに置く。ルートと `kakoi-core` は
  `version.workspace = true` で継承する。仕様の第 17 節「版の典拠がソースコードの 2 か所にある」を
  反例にしないため。
- ルートのパッケージは `src/lib.rs` を持ち続け、CLI 側のモジュールを公開する。`tests/cli.rs`、
  `tests/layers.rs`、`tests/common/fixture.rs` が `kakoi::cli` を使うため、これが無いと結合テストから
  `cli` に届かない。

クレート配置: ルートのパッケージを `kakoi` バイナリのまま残し、`crates/kakoi-core` を workspace の
メンバーとして足す。ルートを仮想 workspace にしないのは、`cargo install --git ... --locked`
（README とドキュメントが案内する導入手順）と、ルートで `cargo build --release` する CI の
リリースジョブをそのまま生かすため。`crates/kakoi-cli` は作らない（`src/main.rs` と同じものに
なる）。テストは全部ルートの `tests/` に残す（`CARGO_BIN_EXE_kakoi` でバイナリを起動するものが
あり、ルートからしか動かせない。`kakoi-core` は自前のテストを持たない）。

## Scope of change

- `Cargo.toml`、`Cargo.lock`（cargo に更新させる。手編集しない）
- `crates/kakoi-core/`（新規）
- `src/`（モジュールの移動と、上の (a)〜(c) の切り離し）
- `tests/`（`use` の先と、テストが組み立てる入力の型の追随のみ。テストが確かめる条件は変えない）
- `.github/workflows/ci.yml`、`lefthook.yml`、`PROJECT.md`（検査コマンドが全メンバーを対象にする
  ように。CI の版の読み取り箇所の追随。実装の置き場所を述べる文の追随）
- Step 5 の検証のための、`crates/kakoi-core/src/` への一時的な編集（元に戻す）

これ以外のファイルは変えない。`docs/spec/kakoi.md`、`README.md`、`docs/*.md`、`CHANGELOG.md`、
`examples/`、`skills/` は対象外。

## Step order and prerequisites

Step 1 → Step 3 の順（3 は 1 の型を使う）。Step 2 は 1 と 3 のどちらとも独立。Step 4 は 1〜3 が
全部済んでいることが前提（core に置くモジュールが CLI 側の型を参照していると移せない）。Step 5 は
4 の後。各ステップは 1 コミット以上の単位で、ステップの終わりで検査コマンドが全部通る。

## Verification map

| 仕様の節 | 確かめるステップ | 何で |
|---|---|---|
| 「## 3. 対応環境」 | 4 | x86_64 以外で `compile_error!` が出る箇所の置き場所を core に保つ |
| 「## 4. コマンドラインインターフェース」「## 12. 入れ子と並列」「## 13. 出力と終了コードの契約」 | 1, 3, 4 | `tests/cli.rs` の全テスト（入れ子と `--print-plan` を含む） |
| 「## 14. 実行時の境界」 | 2, 4 | `tests/launch.rs` の全テスト（記述子の受け渡し、開けるファイル数の上限、ファイルを書かないこと）と `tests/cli.rs` の入れ子の上限のテスト |
| 「## 15. 検証の契約」 | 全ステップ | 検査コマンド 4 つが通る |
| 「## 17. 版とリリース」 | 4, 5 | `--version` の出力と `Cargo.toml` の版が一致し、CI が版を読み取れる |
| 「## 20. 未決と委譲」 | 全ステップ | 観測可能な振る舞いを足さない（テストの追加なし、既存テストの条件の変更なし） |

本計画は振る舞いを変えないリファクタなので、新しいテストは足さない。証拠は「変更前に通る全テストが
変更後も通り、1 つも消えていない」こと。実装者は Step 1 の前に `cargo test --all-targets --locked`
の各 `test result` 行の passed の合計を記録し、各ステップの終わりで同じ数であることを確かめる
（2026-09-10 のこのマシンでの合計は 304。数が違えば記録した方を正とする）。テストは全部ルートの
`tests/` にあるので、Step 4 以降も同じコマンド（Step 5 からは `--workspace` 付き）で数える。

## Left to the implementer（計画全体）

- 各モジュールの名前と、core の中でのファイル配置。
- `kakoi-core` の `Cargo.toml` の `publish` の設定（未設定のままでも `false` でもよい。crates.io に
  出す時期は本計画の範囲外）と `description`。
- 依存の版の固定を `[workspace.dependencies]` にまとめるか、各クレートに書くか。ただし今の
  `=` による完全固定は保ち、まとめる場合はインラインテーブル（`clap = { version = "=4.6.6", ... }`
  の形）で書く（`[workspace.dependencies.clap]` のテーブル形式は `version = ` が行頭に来て、Step 4 の
  版の検査と紛れる）。

## Stop conditions

一般の 4 条件（意味の欠落または承認内容からの逸脱、不可逆・特権・危険な操作、広がる事故、方針転換後の
無進展）に加えて:

- あるテストの `use` の先や入力の組み立て以外（assert する条件、期待する出力、期待する終了コード）を
  変えないと通らない場合。振る舞いが変わった印なので止める。
- core に置くべきモジュールが `clap` か `cli` の型なしに成り立たない場合。
- Rust 1.85 で workspace の継承構文（`version.workspace = true` など）が通らない場合（通る想定。
  通らなければ CI の msrv ジョブが落ちる）。
- `cargo install --git` でルートのバイナリが入らない場合。

## Test command

PROJECT.md が固定している 4 つ。Step 4 までは今の形で、Step 5 で `--workspace`（fmt は `--all`）を
足した形に変わる。

```text
cargo build --locked
cargo test --all-targets --locked
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
```

`tests/launch.rs` と `tests/cli.rs` は `bwrap` 0.9.0 以上、`git`、`python3` を要求する（PROJECT.md）。

## Out of scope

- `crates/kakoi-net`。作らない。空のクレートも置かない。
- `kakoi-core` の公開 API を安定版として宣言すること、crates.io への公開、二人目の利用者に合わせた
  API 設計、`cargo package` / `cargo publish` が通ること（ルートの `include` は変えない）。
- `kakoi-core` 自前のテスト。テストは全部ルートの `tests/` に残す。
- `--print-plan` の出力、診断の文言、終了コード、bwrap に渡す引数列の変更。
- 仕様の改訂。版の引き上げと `CHANGELOG.md` の項目（利用者に見える変更が無いため足さない）。
- 既存テストの条件の変更、テストの追加。

---

## Step 1 — `layers` が CLI の型に依存しない

Purpose: 段の読み込みが受け取る入力を core 側の型にし、`src/layers.rs` から `cli` への依存を切る。
Specification: `docs/spec/kakoi.md`「## 5. ポリシーファイルの形式と合成」「## 20. 未決と委譲」。
Prerequisites: なし。変更前のテスト数を記録しておく。
May change: `src/layers.rs`、`src/cli.rs`、`src/startup.rs`、`src/init.rs`、`src/plan_text.rs`、
`tests/common/fixture.rs`、`tests/layers.rs`、`tests/mounts.rs`、`tests/placement.rs`、`tests/plan.rs`
（テストは入力の組み立てのみ）。
Done when: `src/layers.rs` が `cli` のどの項目も参照せず、段の読み込みに要る値（プロファイル名、
kakoi の `--policy-file` の値、`--rw` の値の列、`--hide` の値の列、既定のプロファイル名）は
`layers` か `policy` が持つ型で受け取り、`cli::Invocation` からその型への変換は CLI 側にある。
`plan_text` が `cli::PlanForm` を使うのはそのまま（`plan_text` は CLI 側に残るため）。
Shown by: check — `grep -n 'cli' src/layers.rs` の結果に `use` の行が無い（依存の不在の本当の証拠は
Step 4 でクレートが分かれてコンパイルが通ること。ここの grep は補助）。続けて検査コマンド 4 つが
通り、passed の合計が記録した数と等しい。
Left to the implementer: 新しい型の名前と置き場所（`layers` か `policy`）、`Invocation` からの変換を
`From` にするか関数にするか、既定のプロファイル名の定数の置き場所。
Stop and hand back if: `layers` が `Invocation` の `print_plan` や `command` や `workspace` を
必要としていた場合（今の `src/layers.rs` が読むのは `profile`、`policy_file`、`rw`、`hide` だけの
はず。違えば設計の前提が違う）。

## Step 2 — `launch` を「組み立て」と「exec」に分ける

Purpose: bwrap の `Command` と exec まで生かすファイル記述子を返す関数を `src/launch.rs` に残し、
exec を CLI 側に移す。
Specification: `docs/spec/kakoi.md`「## 14. 実行時の境界」「## 20. 未決と委譲」。
Prerequisites: なし。
May change: `src/launch.rs`、`src/main.rs`、`src/startup.rs`、CLI 側の新しいモジュール 1 つ（exec を
置く場合。`main.rs` に直接置くなら作らない）。
Done when:
- `src/launch.rs` が `startup` のどの項目も参照せず、`Plan` から `Command`（引数列、`env_clear`
  後の環境、bwrap のパス）と記述子の束を返す関数を持つ。
- 開けるファイル数の soft 上限を hard 上限へ上げる処理は `src/launch.rs` の組み立て側に残る
  （第 14 節「常に上げる」を保つため）。組み立て関数は `Prepared` の経路からだけ呼ばれ、入れ子の
  経路（`Outcome::Nested`）からは呼ばれない（同節「入れ子は上げない」を保つため）。
- 記述子は返り値が生きている間は閉じられず、exec が引き継ぐ。
- 記述子を作れなかったときの種類 `bwrap` の診断は、文言を変えずに組み立て側が返す。
- exec と、exec が失敗したときの種類 `bwrap` の診断は CLI 側（`main.rs` か CLI 側の新しい
  モジュール）にある。`src/launch.rs` に `.exec()` の呼び出しが無い。
Shown by: check — `grep -n 'startup\|exec()' src/launch.rs` の結果に `use` の行と `.exec()` の
呼び出しが無い（文書コメントの語は無視する）。検査コマンド 4 つが通り、passed の合計が記録した数と
等しい。特に `tests/launch.rs` の `the_isolated_process_inherits_the_raised_soft_limit`、
`an_rw_copy_file_is_written_inside_and_the_host_file_is_untouched`、
`an_rw_copy_directory_carries_the_host_tree_and_keeps_every_change_inside` と、`tests/cli.rs` の
`a_nested_launch_leaves_the_soft_limit_unchanged` が通る。
Left to the implementer: 返り値の型の名前と形（`Command` と `Vec<OwnedFd>` を持つ構造体か、組か）、
exec を `main.rs` に直接書くか CLI 側のモジュールに置くか、そのモジュールの名前。
Stop and hand back if: 記述子を `Command` の外で持つと exec 前に閉じられてしまう構造にしかならない
場合（今の実装は `descriptors` を exec まで生かしている。それが保てないなら止める）。

## Step 3 — 段 4〜9 を、環境を値で受ける関数にする

Purpose: `startup::prepare` のうち、段の読み込みから計画までを、環境変数の写しとカレント
ディレクトリを引数で受け取る core 側の関数にし、引数の解釈、入れ子の判定、カレントディレクトリの
取得（段 1〜3）を CLI 側に残す。
Specification: `docs/spec/kakoi.md`「## 13. 出力と終了コードの契約」（段の順序）、
「## 12. 入れ子と並列」「## 4. コマンドラインインターフェース」「## 14. 実行時の境界」（起動された
環境を信頼すること）、「## 20. 未決と委譲」。
Prerequisites: Step 1（段の読み込みの型がある）。
May change: `src/startup.rs`、`src/environment.rs`、`src/cli.rs`、`src/main.rs`、core 側に置く
新しいモジュール 1 つ。
Done when:
- 段 4〜9（ホームディレクトリ、段の読み込みと合成、作業場所と 4 変数、マウントの解決、`bwrap` の
  所在、command の解決、計画）を行う関数が新しいモジュールにあり、次を引数で受け取る: Step 1 の
  段の読み込みの型、kakoi の `--workspace` の値、command とその引数、カレントディレクトリ、環境変数の
  写し（`BTreeMap<OsString, OsString>`）。
- その関数と新しいモジュールは `std::env::vars_os()`・`std::env::var_os`・`std::env::current_dir()`
  を呼ばず、`cli` のどの項目も参照しない。`HOME` と `XDG_CONFIG_HOME` は環境変数の写しから取る
  （`HostEnvironment::from_process` は CLI 側に移すか、写しから組み立てる構築子に置き換える。
  `src/environment.rs` に `std::env` の呼び出しを残さない）。
- 段 3（`std::env::current_dir()` の取得と、失敗したときの種類 `path` の診断）と、`Invocation` の
  相対パスをカレントディレクトリに固定する `anchored` は、CLI 側で、この関数を呼ぶ前に済ませる。
- `startup::prepare` は段 1〜3 を行い、この関数を呼ぶ。診断の出る順は今と同じ。
- 入れ子で `--print-plan` のときに段 4〜9 を通り、command をホストの `PATH` で解決する振る舞い
  （第 4.2 節、第 12.1 節）は保つ。
Shown by: check — `grep -rn 'std::env' src/environment.rs <新しいモジュール>` が何も出さず、
`grep -n 'cli' <新しいモジュール>` の結果に `use` の行が無い。検査コマンド 4 つが通り、passed の
合計が記録した数と等しい。特に `tests/cli.rs` の
`a_nested_print_plan_reads_the_policy_and_marks_the_plan_as_nested` と入れ子の各テスト、
`tests/launch.rs` の `a_nested_launch_runs_under_the_outer_boundary` が通る。
Left to the implementer: 関数とモジュールの名前、入力をまとめる構造体を作るか引数で並べるか、
`HostEnvironment` を写しから作る構築子の形。
Stop and hand back if: 段の順序（第 13 節）を保ったまま分けられず、診断の出る順が変わる場合。

## Step 4 — workspace 化と `crates/kakoi-core` への移動

Purpose: ルートを `kakoi` バイナリのパッケージのまま workspace にし、CLI でないモジュールを
`crates/kakoi-core` に動かし、CI の版の読み取りを追随させる。
Specification: `docs/spec/kakoi.md`「## 3. 対応環境」「## 17. 版とリリース」「## 20. 未決と委譲」。
Prerequisites: Step 1〜3。
May change: `Cargo.toml`、`Cargo.lock`、`crates/kakoi-core/`（新規）、`src/`、`tests/`（`use` の先
のみ）、`.github/workflows/ci.yml`（リリースジョブの版を読む awk のみ）。
Done when:
- ルートの `Cargo.toml` に `[workspace]`（`members = ["crates/kakoi-core"]`）と
  `[workspace.package]`（`version`、`edition`、`rust-version`、`license`、`repository`）があり、
  ルートの `[package]` と `crates/kakoi-core` の `[package]` はそれらを `workspace = true` で
  継承する。版の文字列は両方の `Cargo.toml` を合わせて `[workspace.package]` の 1 行だけにある。
  ルートの `include` は変えない。
- `crates/kakoi-core` は `serde`、`toml`、`libc` に依存し、`clap` と `serde_json` に依存しない。
  ルートは `clap`、`serde`、`serde_json`、`kakoi-core`（`path = "crates/kakoi-core"`）に依存し、
  `tests/seccomp.rs` と `tests/launch.rs` が使う `libc` を `[dev-dependencies]` に持つ。
- core に動かすモジュール: `policy`、`layers`、`variables`、`wildcard`、`environment`、
  `workspace_facts`、`regular_file`、`scan`、`mounts`、`mount_list`、`mount_facts`、`placement`、
  `isolated_env`、`secret_facts`、`copies`、`copy_facts`、`executables`、`command`、`seccomp`、
  `plan`、`diagnostic`、`launch`（Step 2 の後の組み立て側）、Step 3 の新しいモジュール。
  ルートに残すモジュール: `cli`、`startup`、`init`、`plan_text`、`plan_json`、`main`、Step 2 で
  exec を置いたモジュール（作った場合）。ルートの `src/lib.rs` は残すモジュールを `pub mod` で公開する。
- `src/layers.rs` の `include_str!` は `../../../examples/profile/default.toml` を指す。
  `examples/profile/default.toml` を複製しない（典拠を 2 つにしない）。
- x86_64 以外を `compile_error!` で止める箇所は `crates/kakoi-core/src/lib.rs` にある。
- `ci.yml` のリリースジョブで版を読む awk が `[workspace.package]` の下の `version` を読む
  （今は `[package]` の下を読み、`version.workspace = true` は拾えず、空だと `exit 1` で落ちる）。
- `Cargo.lock` は cargo が更新したもので、`--locked` の検査が通る。
- `tests/` の各ファイルは `use` の先が変わるだけで、テストの本体は変わらない。
Shown by: check — 検査コマンド 4 つに `--workspace`（fmt は `--all`）を付けて通り、passed の合計が
記録した数と等しい。`grep -rn 'clap\|serde_json\|std::env\|\.exec()\|println!\|eprintln!\|stdout()\|stderr()' crates/kakoi-core/`
が何も出さない。`grep -n '^version = "' Cargo.toml crates/kakoi-core/Cargo.toml` の結果が
`[workspace.package]` の下の 1 行だけ。`ci.yml` の awk の部分をそのまま手元で `Cargo.toml` に対して
走らせ、`0.3.0` が出る。一時ディレクトリへ `cargo install --git <このリポジトリのパス> --locked
--root <一時ディレクトリ>` が `kakoi` を入れ、`<一時ディレクトリ>/bin/kakoi --version` が
`kakoi 0.3.0` を出す（`--git` はローカルのパスも受け付ける。コミット済みの状態を見るので、確かめる
前にコミットする）。
Left to the implementer: core の `lib.rs` でモジュールをどう公開するか（今と同じ `pub mod` の並びで
よい。再輸出の整理は本計画の範囲外）、awk の書き方（節の名前を変えるだけでよい）。
Stop and hand back if: どれかのモジュールが core と CLI の両方から要るのに、どちらか片側にしか
置けない場合。`cargo install --git` がルートのパッケージを選ばない場合。`include_str!` の相対
パスが `cargo install --git` の取り出しで解決できない場合。

## Step 5 — 検査コマンドと CI と hook を全メンバー対象にする

Purpose: 文書と CI と pre-commit hook の検査コマンドが `kakoi-core` も対象にするようにする。
Specification: `docs/spec/kakoi.md`「## 15. 検証の契約」。
Prerequisites: Step 4。
May change: `PROJECT.md`（検査コマンドの節と、実装が `src/` にあると述べる文）、
`.github/workflows/ci.yml`（`check` ジョブと `msrv` ジョブのコマンド）、`lefthook.yml`。検証のための
`crates/kakoi-core/src/` への一時的な編集（元に戻す）。
Done when:
- PROJECT.md の 4 つのコマンドが `cargo build --workspace --locked`、
  `cargo test --workspace --all-targets --locked`、`cargo fmt --all --check`、
  `cargo clippy --workspace --all-targets --locked -- -D warnings` になっている。実装の置き場所を
  述べる文が `src/` と `crates/kakoi-core/src/` の両方を挙げる。
- `ci.yml` の `check` ジョブと `msrv` ジョブの同じコマンドに `--workspace`（fmt は `--all`）が
  足されている。既にある `--target ${{ matrix.target }}` はそのまま。リリースジョブの
  `cargo build --release` はルートのバイナリを作るので変えない。
- `lefthook.yml` の `fmt` と `clippy` に同じフラグが足され、`glob` が `crates/` の下の `.rs` にも
  一致する。
Shown by: check — `crates/kakoi-core/src/` のどれかに、わざと clippy の警告になる行（例: 使わない
変数）を一時的に足し、PROJECT.md の clippy のコマンドが失敗することを見る。次にわざと整形の崩れを
足し、fmt のコマンドが失敗することを見る。どちらも確かめたら元に戻し、`git status` で作業ツリーが
その一時編集を含まないことを見る。最後に検査コマンド 4 つ（新しい形）が通る。
Left to the implementer: none。
Stop and hand back if: `cargo fmt --all` が `crates/kakoi-core` を対象にしない場合（rustfmt の版に
よる。その場合は fmt の対象の指定方法を計画に戻して決める）。lefthook の `glob` の書き方で
`crates/` の下に一致させられない場合。
