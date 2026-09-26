# Plan: 隔離の中で起動されるプログラムの使い方を止めるガードレールを作る

## Goal

ポリシーに "[[commands.guard]]" の規則を書いた利用者が、隔離の中で LLM が PATH から起動したプログラム（例: git）の禁じた使い方（例: push）を、理由の 1 行と終了コード 126 で止められ、その規則と見張り役の置き場所を `--print-plan` で確かめられる状態にする。

## Specification

IR は `docs/ir/`。根拠の決定記録は `docs/decision/brainstorm/2026-09-25-command-policy.md`（A1〜A37、D1）。

- 規則: `docs/ir/core/core-command-guard-rules.md#REQ-438`〜`#REQ-444`（REQ-445 は欠番）
- 見張り役: `docs/ir/core/core-command-guard-runtime.md#REQ-446`〜`#REQ-454`
- 既存の要求で今回句を足したもの: `docs/ir/core/core-policy.md#REQ-151` と `#TBL-152`、`docs/ir/core/core-terms.md#REQ-195`、`docs/ir/core/core-environment.md#REQ-268`、`docs/ir/core/core-output.md#REQ-290`、`docs/ir/core/core-plan-output.md#REQ-299`、`docs/ir/core/core-product.md#REQ-353`
- 例: EX-846〜EX-883 のうち上の 2 文書にあるもの

各要求と例の本文は `kotowari query REQ-nnn`、`kotowari query EX-nnn` で読む。

## Approach and why

責務の分け方は REQ-454 に従う。規則の型、読み込み、段の合成、語の照合、例の検証、見張り役を置く計画は kakoi-core に置き、見張り役として起動されたときの処理（場所の対応を読む、引数と環境を受け取る、禁止の 1 行を出す、本物を exec する）は kakoi の実行ファイル（`src/`）に置く。例の検証と見張り役は kakoi-core の同じ照合の関数を使う。新しいクレートは作らない。

- 規則は `crates/kakoi-core/src/policy.rs` の既存の型に倣って `serde` の `deny_unknown_fields` で読み、形の誤り（REQ-438）は読み込みの段階で `policy` の診断にする。段の合成は `crates/kakoi-core/src/layers.rs` のリストの連結と同じ扱いにする（TBL-152）。
- 照合は入出力の無い関数として kakoi-core の新しいモジュールに置く。例の検証はポリシーの読み込みと合成の段階で行う（A32）。
- 見張り役を置く計画は、既存の計画算出（`crates/kakoi-core/src/planning.rs`、`plan.rs`）の中で、環境の組み立て（`crates/kakoi-core/src/isolated_env.rs` の `assemble_environment`、`resolve_isolation` の中）とマウントの解決の後に行う。組み立てた環境の PATH（見張り役の場所はまだ無い）で本物を探し、REQ-449 の条件で置くか飛ばすかを決め、置くものが 1 つ以上あり PATH があるときだけ、PATH の最も先頭に見張り役の場所を足す。すべて飛ばしたときは足さない。本物は REQ-446 のとおり、シェルがその名前で起動するもの（その名前の場所もリンクを解決した実体も隠されず、リンクを解決した先が実行できる通常ファイルである最初の名前）として探す（決定記録 A38、A44。当初はリンクをたどった先が通常ファイルでない最初の名前で止めて飛ばしていたが、後ろの本物が見張られないので改めた）。実体が隠されるかの判定には、リンクを解決した実体のパスを使う（マウントの規則 REQ-169 と同じ）。
- kakoi に直接渡したコマンド（REQ-446、A20）は、名前で PATH から探した結果が見張り役を置いたプログラムの本物なら、bwrap が起動するパスを見張り役の場所に置き換える。argv[0] は与えた名前のまま。`/` を含むコマンドは、`guard-absolute-path` の規則で重ねた本物の場所に当たるときだけ見張り役を通る。入れ子の起動と filtered の起動でも同じ置き換えにする。計画表示の command の path には、置き換えた後のパスを出す。
- 隔離の中への配置は、bwrap の引数を作る `crates/kakoi-core/src/plan.rs` の `bwrap_arguments` で、利用者のマウント（と filtered の resolv.conf）の後に足す。`--ro-bind / /` の後には新しい最上位のディレクトリを作れないので、見張り役の場所は bwrap が作る tmpfs（`/dev` など）の下に kakoi 専用の tmpfs として作り、中身を置いた後で `--remount-ro` する。ホストの内容を持つディレクトリには重ねない。kakoi 自身の実行ファイルは、プログラムごとに別の bind で置く（`/proc/self/exe` で見分けるため。シンボリックリンクにしない）。bind 元は、`src` がホストで得た kakoi 自身のパス（`std::env::current_exe`）か、`/proc/self/exe` を開いた記述子で、事実として kakoi-core に渡す。bwrap の引数に `/proc/self/exe` という文字列を書かない（bwrap 自身を指す）。規則と場所の対応は memfd の記述子を記号の `Argument` として持ち、`launch.rs` が番号に置き換える（resolv.conf の `CopiedFile` と同じ扱い）。規則と対応は見張り役と同じ tmpfs の中に置く。見張り役の引数にリテラルの `--` を入れない（`crates/kakoi-net/src/application.rs` が最初の `--` を目印に引数を書き換えるため）。`guard-absolute-path` の規則は本物の実体のパスに見張り役を重ね、本物をその tmpfs の中に読み取り専用で置き直す。置き場所のパスの具体値は実装が決める（D1）。
- 見張り役としての起動の判定は `src/main.rs` の最初、`kakoi_net::init::run_if_requested` と `startup::prepare` より前に置く（REQ-453）。起動された実行ファイルの場所（`/proc/self/exe`）が場所の対応に載っているときだけ見張り役として動く。対応は「場所 → その場所に当てるプログラムの規則すべて」の形にし、同じ本物を複数の名前が指すとき（`guard-absolute-path` で 2 つの名前の本物が同じ実体の場合）に、それらすべての規則を当てる。
- 計画表示は `src/plan_text.rs` と `src/plan_json.rs` に足す（REQ-450、REQ-299 の `guards` と `skipped_guards`）。

テストは実装の振る舞いではなく仕様を満たすかを確かめる。例ごとに、実装を読む前に仕様（例の Given/When/Then と `@about` の要求の本文）だけから作る条件と期待する結果を書き出し、それに合うテストを書く。Then の各句をすべて主張し、仕様が契約として宣言していない細部（説明文の言い回し、内部の名前、置き場所のパスの具体値）は主張しない。診断は `tests/common` の `assert_diagnostic`（種類、終了コード、標準出力が空）で確かめる。見張り役の禁止の 1 行は、REQ-447 が形を契約として定めているので、その形で確かめる。

観測の境界は次のとおり。

- REQ-439〜REQ-443 の照合の規則は、kakoi-core の公開された照合の関数を製品の境界として確かめてよい（kakoi-core はライブラリとして公開されている）。
- それ以外（読み込みの誤り、例の検証、計画、見張り役の振る舞い）は、組み立てた `kakoi` の実行ファイルと実際のファイルシステムで確かめる。規則を当てるプログラムは、テストの一時ディレクトリに置いた小さなスクリプト（受け取った引数を出すもの）でよい。本物の git に頼らない。
- 時間に依存するテストは書かない。
- 例に出てくるパス（`/usr/bin/git`、`/opt/tools/bin`、`/w/sub` など）は例の値で、テストではテストの一時ディレクトリの中のパスに置き換え、Then の値も置き換えた値で主張する。
- 通過の例（EX-867、EX-879、EX-882）は、見張り役が介在しなくても同じ結果になるので、同じ設定で見張り役が介在していることも併せて確かめる（隔離の中の `command -v <名前>` が `--print-plan=json` の `guards` にある見張り役の場所と一致する、同じ規則で禁止の起動が 126 になる、など）。EX-882 のコマンドは PATH 経由で起動させる。
- 例の無い要求の句は、次のテストで確かめる。REQ-439 の UTF-8 として読めない語は当たらない（照合の関数へのテスト）。REQ-447 の制御文字を見える表記に逃がす（引数に制御文字を含む禁止の起動）。REQ-448 の同じ環境（隔離の中で決めた変数が本物に届く）。REQ-449 の本物が kakoi 自身であるとき飛ばす（計画表示）。REQ-446 の本物の探し方（前にある同名のディレクトリ、行き先の無いリンク、実行できないファイル、場所か実体が隠される名前を飛ばす）。REQ-453 の同じ本物を複数の名前が指すとき（2 つの名前と `guard-absolute-path` で、両方の規則が当たる）。隔離の外へ通知しないことは、仕組みが無いことをレビュー（S7）で確かめる。

テストの印は kotowari の mark の場面に従い、テスト関数の直前に `// @kotowari[REQ-nnn, EX-nnn]` の形で置く。

## Scope of change

- `crates/kakoi-core/src/`（`policy.rs`、`layers.rs`、`isolated_env.rs`、`planning.rs`、`plan.rs`、`launch.rs`、`diagnostic.rs`、`lib.rs`、新しい照合のモジュール）
- `src/`（`main.rs`、`plan_text.rs`、`plan_json.rs`、見張り役の処理の新しいモジュール、`lib.rs`）
- `crates/kakoi-core/src/`（上に加えて `command.rs`、`executables.rs`）、`crates/kakoi-net/src/application.rs`（引数の書き換えが見張り役の引数と両立するよう、要る範囲だけ）
- `Cargo.toml`（`[workspace.dependencies]` に `regex` と、使うなら `shlex` を厳密な版で固定する）、`crates/kakoi-core/Cargo.toml`、`Cargo.lock`
- `tests/`（`tests/cli.rs`、`tests/launch.rs`、`tests/plan.rs`、`tests/policy.rs`、`tests/layers.rs`、`tests/isolated_env.rs`、新しい `tests/guard.rs`、`tests/common/`、`tests/kakoi_net/cli.rs`、`tests/kakoi_net/supervisor.rs`、`tests/kakoi_net/application.rs`）
- `PROJECT.md`（テストが隔離の中で起動するものの記述に要る範囲）
- `examples/profile/default.toml`（コメントにした見本）
- `docs/policy.md`、`docs/security.md`、`docs/cli.md`（JSON のキーと診断の一覧に要る範囲）
- `docs/guide/`（今回の要求の節の新設と、古くなった節の読み直し）

## Step order and prerequisites

S1、S2、S3 の順に行う（規則の型の上に照合を、照合の上に例の検証を置く）。S4 は S1 の後。S5 は S2 と S4 の後。S6 は S5 の後。S7 は最後。

## Verification map

| Step | 要求 | 例 |
|---|---|---|
| S1 | REQ-438、REQ-151、TBL-152 | EX-846、EX-847、EX-871 |
| S2 | REQ-439、REQ-440、REQ-441、REQ-442、REQ-443（照合の順） | EX-848〜EX-858、EX-872〜EX-874 |
| S3 | REQ-444、REQ-443（合成） | EX-859、EX-860、EX-875、EX-876、EX-861 |
| S4 | REQ-446（計画）、REQ-449、REQ-450、REQ-268、REQ-299 | EX-868、EX-869、EX-870、EX-883 |
| S5 | REQ-446（配置）、REQ-447、REQ-448、REQ-453、REQ-290、REQ-195 | EX-862〜EX-867、EX-877〜EX-882 |
| S6 | REQ-451、REQ-452、REQ-353（レビュー）、ガイド | なし |
| S7 | REQ-454（レビュー）、全体の確認 | なし |

## Left to the implementer

- 新しいモジュールと型と関数の名前
- 見張り役、置き直した本物、規則と場所の対応の置き場所と、渡し方の細部（D1。読み取り専用で、見張り役が kakoi 自身の実行ファイルであることは守る）
- 計画表示の文面（要約と全量）。JSON のキー名は REQ-299 と REQ-450 のとおり `guards` と `skipped_guards`

## Stop conditions

- 仕様どおりのテストが製品で失敗し、仕様と実装のどちらを直すかが明らかでない
- 見張り役を置くのに bwrap 0.9.0 でできない操作が要る
- 見張り役としての起動の判定を、入れ子の判定より前に置けない
- `guard-absolute-path` の重ね方が、利用者のマウントや rw-copy の tmpfs と両立しない
- 既存のテストが、この変更と関係のない理由で失敗する

## Test command

```sh
cargo test --workspace --all-targets --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

## Out of scope

- LLM による判定（ba-toys の guardrail、A11）
- 書ける場所に置いたプログラムの実行の禁止（A5）
- CHANGELOG（版を切るときに書く）

## Steps

### S1: 規則を読み込み、段をまたいで合成する

- Purpose: "[[commands.guard]]" を固定キーとして読み、形の誤りを policy の診断にし、段をまたいで連結する
- Specification: `docs/ir/core/core-command-guard-rules.md#REQ-438`、`docs/ir/core/core-policy.md#REQ-151`、`docs/ir/core/core-policy.md#TBL-152`
- Prerequisites: none
- May change: `crates/kakoi-core/src/policy.rs`、`crates/kakoi-core/src/layers.rs`、`crates/kakoi-core/src/lib.rs`、新しい照合のモジュール（型だけ）、`tests/policy.rs`、`tests/layers.rs`、`tests/guard.rs`、`tests/common/`、既存の型に項目を足したことでコンパイルが通らなくなる既存のテスト
- Done when: EX-846、EX-847、EX-871 の条件で読み込みが仕様どおり止まるか通る
- Shown by: test — 例ごとのテストを実行ファイルの `--print-plan` で書く
- Left to the implementer: none
- Stop and hand back if: 既存の固定キーの検査と両立しない

### S2: 語の照合を作る

- Purpose: 先頭一致、読み飛ばし、フラグ、オプションの値、環境変数、for、照合の順を、入出力の無い関数として kakoi-core に作る
- Specification: `docs/ir/core/core-command-guard-rules.md#REQ-439`、`docs/ir/core/core-command-guard-rules.md#REQ-440`、`docs/ir/core/core-command-guard-rules.md#REQ-441`、`docs/ir/core/core-command-guard-rules.md#REQ-442`、`docs/ir/core/core-command-guard-rules.md#REQ-443`
- Prerequisites: S1
- May change: `crates/kakoi-core/src/` の照合のモジュール、`Cargo.toml`、`crates/kakoi-core/Cargo.toml`、`Cargo.lock`、`tests/guard.rs`
- Done when: EX-848〜EX-858 と EX-872〜EX-874 のそれぞれと、REQ-439 の UTF-8 として読めない語の句が、kakoi-core の公開された照合の関数へのテストで仕様どおりの禁止と非禁止になる
- Shown by: test — 例ごとに 1 つのテスト。RED は関数が無いことか誤った結果、GREEN で通す
- Left to the implementer: 正規表現のクレートの版（Rust の regex クレートの構文であること、A35）
- Stop and hand back if: 例どうしが同時に満たせない

### S3: 例を読み込み時に検証する

- Purpose: 規則の例をポリシーの読み込みと合成の段階で照合し、期待と違えば policy の診断で止める
- Specification: `docs/ir/core/core-command-guard-rules.md#REQ-444`、`docs/ir/core/core-command-guard-rules.md#REQ-443`
- Prerequisites: S2
- May change: `crates/kakoi-core/src/`（読み込みの段階と照合のモジュール）、`Cargo.toml`、`crates/kakoi-core/Cargo.toml`、`Cargo.lock`、`tests/guard.rs`
- Done when: EX-859、EX-860、EX-875、EX-876 が実行ファイルで仕様どおりになり、EX-875 はホストの環境に `GIT_CONFIG_COUNT` があっても通り、EX-861 は 2 つの段にそれぞれ置いた規則の `examples.deny`（`git push` と `git fetch`）が合成後の例の検証を通ることで確かめられる
- Shown by: test — 例ごとのテストを実行ファイルで書く。EX-859 は説明に期待と違った例の文字列が含まれることを確かめる
- Left to the implementer: 例の語の分け方の実装（シェルと同じ引用の規則であることは決まっている）
- Stop and hand back if: 例の語の分け方に、`Cargo.lock` に無い新しい依存が要る（`shlex` は既に推移的な依存として入っている）

### S4: 見張り役を置く計画と計画表示を作る

- Purpose: 本物を探し、置くか飛ばすかを決め、PATH の先頭に見張り役の場所を足し、計画表示と JSON に載せる
- Specification: `docs/ir/core/core-command-guard-runtime.md#REQ-446`、`docs/ir/core/core-command-guard-runtime.md#REQ-449`、`docs/ir/core/core-command-guard-runtime.md#REQ-450`、`docs/ir/core/core-environment.md#REQ-268`、`docs/ir/core/core-plan-output.md#REQ-299`
- Prerequisites: S1
- May change: `crates/kakoi-core/src/planning.rs`、`crates/kakoi-core/src/plan.rs`、`crates/kakoi-core/src/isolated_env.rs`、`crates/kakoi-core/src/command.rs`、`crates/kakoi-core/src/executables.rs`、`src/startup.rs`、`src/plan_text.rs`、`src/plan_json.rs`、`tests/guard.rs`、`tests/plan.rs`、`tests/cli.rs`、`tests/isolated_env.rs`、`tests/kakoi_net/supervisor.rs`、`tests/kakoi_net/application.rs`
- Done when: EX-868、EX-869、EX-870、EX-883 が `--print-plan` と `--print-plan=json` で仕様どおりになり、EX-868 は同じ規則で `kakoi -- /bin/true` が 0 で終わり、見張り役を置くときの隔離の中の PATH の最初の項目が見張り役の場所でその後ろは元の PATH のままで、すべて飛ばしたときは PATH が変わらず、kakoi 自身のときは理由付きで飛ばされる
- Shown by: test — 例ごとのテストを実行ファイルで書く。PATH は `--print-plan=json` の環境か、隔離の中で PATH を出すコマンドで確かめる
- Left to the implementer: 計画表示の文面
- Stop and hand back if: 本物を探す PATH の定義（A28）と既存のコマンドの解決（REQ-260）が食い違う、または隠されるかの判定に使うパスが既存のマウントの規則と両立しない

### S5: 見張り役を隔離の中に置き、起動のときに当てる

- Purpose: bwrap の引数で見張り役と規則と対応を置き、見張り役としての起動を見分け、禁止なら 1 行と 126、通過なら本物を exec する
- Specification: `docs/ir/core/core-command-guard-runtime.md#REQ-446`、`docs/ir/core/core-command-guard-runtime.md#REQ-447`、`docs/ir/core/core-command-guard-runtime.md#REQ-448`、`docs/ir/core/core-command-guard-runtime.md#REQ-453`、`docs/ir/core/core-output.md#REQ-290`、`docs/ir/core/core-terms.md#REQ-195`
- Prerequisites: S2、S4
- May change: `crates/kakoi-core/src/plan.rs`、`crates/kakoi-core/src/planning.rs`、`crates/kakoi-core/src/launch.rs`、`crates/kakoi-core/src/diagnostic.rs`、`crates/kakoi-net/src/application.rs`、`src/main.rs`、`src/lib.rs`、`src/startup.rs`、`src/plan_text.rs`、`src/plan_json.rs`、見張り役の処理の新しいモジュール、`tests/guard.rs`、`tests/launch.rs`、`tests/plan.rs`、`tests/kakoi_net/cli.rs`、`tests/kakoi_net/application.rs`
- Done when: EX-862〜EX-867 と EX-877〜EX-882 が実行ファイルで仕様どおりになり、host と none と filtered（`tests/kakoi_net/cli.rs` の偽の pasta の仕組みで 1 つ）で見張り役が禁止する
- Shown by: test — 例ごとのテストを実行ファイルで書く。EX-866 と EX-881 は標準エラーの 1 行を REQ-447 の形どおりに確かめる。EX-879 は本物のプログラムの出力が返り、入れ子の警告が無いことを確かめる。EX-865 は、`--print-plan=json` の `guards` に出る見張り役の場所の tmpfs の中のすべてのファイルと、そのディレクトリ自身へ書き込もうとして、どれもできないことで確かめる（規則と対応は見張り役と同じ tmpfs の中に置く）。例の無い句は Approach のとおり確かめる
- Left to the implementer: 見張り役、置き直した本物、規則と対応の置き場所と渡し方（D1）
- Stop and hand back if: bwrap 0.9.0 の引数で置けない、または見張り役の判定を入れ子の判定より前に置けない

### S6: 同梱の見本、公開文書、ガイドを書く

- Purpose: コメントにした git の見本を同梱プロファイルに入れ、公開文書とガイドにガードレールを書く
- Specification: `docs/ir/core/core-command-guard-runtime.md#REQ-451`、`docs/ir/core/core-command-guard-runtime.md#REQ-452`、`docs/ir/core/core-product.md#REQ-353`
- Prerequisites: S5
- May change: `examples/profile/default.toml`、`docs/policy.md`、`docs/security.md`、`docs/cli.md`、`docs/guide/`、`PROJECT.md`
- Done when: REQ-451 と REQ-452 の how_to_verify の内容を満たし、見本のコメントを外した写しで kakoi が起動して例の検証を通り、`kotowari check` がガイドに guide_stale も invalid_marker も出さない
- Shown by: check — 見本のコメントを外した写しを一時ディレクトリに置き、`kakoi --policy-file <写し> --print-plan` が 0 で終わること、`kotowari check --format json | jq '[.findings[] | select(.path | startswith("docs/guide/"))]'` が空であること
- Left to the implementer: 文書とガイドの構成と言い回し（仕様が挙げる内容を満たす範囲）
- Stop and hand back if: ガイドに書くために IR に無い規則が要る

### S7: 全体を確かめる

- Purpose: この計画の要求と例がそろってテストで覆われ、責務の分け方が仕様どおりであることを示す
- Specification: `docs/ir/core/core-command-guard-runtime.md#REQ-454`
- Prerequisites: S1、S2、S3、S4、S5、S6
- May change: none
- Done when: Test command がすべて通り、この計画の要求と例のうちレビューで確かめるもの以外の `kotowari query` の tests が空でなく、`kotowari check` の誤りが 0 で、REQ-454 の how_to_verify に沿った読み取りで責務の分け方が仕様どおりで、見張り役に隔離の外へ通知する仕組みが無い（REQ-447）
- Shown by: check — Test command の 3 つ、`kotowari check` の終了コードが 0 であること、REQ-454 の how_to_verify に沿って crates/kakoi-core と src を読んだ結果
- Left to the implementer: none
- Stop and hand back if: 誤りが残る
