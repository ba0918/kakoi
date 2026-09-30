# Plan: 入れ子の中でも、呼ぶ側が頼めば隔離を作り直す

## Goal

kakoi の隔離の中で動くツールが `kakoi --nested=isolate ...` を起動すると、入れ子でも中で隔離が作られて外と中の制限（ファイル、通信、外のガードレール）が重なり、外の policy が許せば中で filtered も使える状態にする。入れ子の判定は環境変数 `KAKOI` ではなく隔離の中の印で行う。

## Specification

IR は `docs/ir/`。決定の記録は `docs/decision/brainstorm/2026-09-29-nested-isolation.md`（A1〜A53、判断待ち U1 と U2）。要求の本文は `kotowari query REQ-nnn`、例は `kotowari query EX-nnn` で読む。本文をこの計画に写さないので、各ステップの前に必ず読むこと。

- 新しい要求: `docs/ir/core/core-nested-isolation.md#REQ-455` 〜 `#REQ-462`、`#REQ-465`、`#REQ-466`、`docs/ir/core/core-cli-syntax.md#REQ-464`
- 新しい例: EX-884〜EX-901（REQ-463 だけを指すものは無い）
- 今回文を改めた既存の要求（今あるテストが古い動きを確かめているので直す）: `docs/ir/core/core-process.md#REQ-284`、`#REQ-285`、`#REQ-287`、`docs/ir/core/core-command-resolution.md#REQ-261`、`#REQ-262`、`#REQ-263`、`docs/ir/core/core-output.md#REQ-290`、`#REQ-293`、`#REQ-401`、`docs/ir/core/core-runtime.md#REQ-305`、`#REQ-306`、`#REQ-308`、`#REQ-309`、`#REQ-314`、`#REQ-315`、`docs/ir/core/core-plan-output.md#REQ-295`、`#REQ-297`、`#REQ-299`、`docs/ir/core/core-cli-syntax.md#REQ-252`
- 今回改めた既存の例と表: EX-499、EX-501、EX-502、EX-520、EX-521、EX-547、`docs/ir/core/core-verification.md` の TBL-158（init の行）
- レビューで確かめる要求: `docs/ir/core/core-nested-isolation.md#REQ-463`、`docs/ir/core/core-process.md#REQ-286`、`docs/ir/core/core-product.md#REQ-353`、`docs/ir/core/core-terms.md#REQ-183`、`#REQ-196`、`docs/ir/core/core-public-docs.md` の TBL-160（行 8 と 17）。EX-522 は REQ-286 だけを指すのでレビューで確かめる

## Approach and why

入れ子の判定を最初に変える。`--nested=isolate`、共有ファイルの置き場、tun、外の見張り役の引き継ぎのどれも「入れ子かどうか」に依存しており、判定が環境変数のままだとテストの前提（入れ子の中にいる）を作れないからである。判定を変えると、今ある入れ子のテストは前提を失う。ホストで `KAKOI=1` を立てて入れ子を装うテスト（`tests/cli.rs` の入れ子の補助関数を使うものなど）と、隔離の中で `env -u KAKOI` を使って中の隔離を作るテスト（`tests/launch.rs` の REQ-286 のもの）である。これらは S1 で「組み立てた kakoi の隔離の中で、組み立てた kakoi をもう一度起動する」形に作り直す。`--nested` を付けない入れ子（`--nested=exec` の入れ子）の振る舞いは S1 で、`--nested=isolate` の振る舞いは S3 以降で確かめる。以降のステップの入れ子のテストもこの形を使う。

ホストのファイルシステムを見る処理（入れ子の印の有無、外の見張り役の置き場の有無、共有ファイルの置き場の作成と確かめ、`/dev/net/tun` の有無）は外側の層（`src/` と、既存の事実を読むモジュール）に置き、`kakoi-core` の計画の組み立てには値として渡す。今の `is_nested` が環境の値を受け取る純粋な関数であるのと同じ分け方で、計画の組み立てをホストに触れずに試せる形を保つ。

置き場の確かめと作り直しは、計画表示が置き場に書かない（`docs/ir/core/core-nested-isolation.md#REQ-461`）ので、計画の組み立ての中では行わない。計画は「置き場を使う形」で組み、実際の起動の直前（コマンドの解決の後、bwrap の起動の前）に置き場を確かめ、使えなければその起動の引数をデータから作る形に差し替える。host と none では `src/main.rs` の起動、filtered では `crates/kakoi-net/src/application.rs` の起動がその場所に当たる。filtered の resolv.conf は今 `application.rs` が `/etc/resolv.conf` の実体のパスへ差し替えているので、置き場の形でも同じ実体のパスに bind する。

`network.allow-nested-filtered` は policy の network の節のキーだが、`docs/ir/network/policy/network-mode.md` の「残る通信許可・公開設定」には当たらない。今の `settings_present`（`crates/kakoi-core/src/policy.rs`、`layers.rs`）に数えると、host と none で「使われない設定」の警告が出て、`network.mode` がどの段にも無いと policy の診断になる。これは REQ-458 の「ネットワークのモードによらず」に反するので、このキーはそこに数えない。

入れ子の filtered の端から端までの確かめ（EX-887、EX-889、EX-891）は、pasta で実際に通信するテストの既存の方式（`unshare` で作った名前空間をホストの代わりにする）の中で、外の kakoi と中の kakoi を重ねて行う。重いので、部品（印、`--nested=isolate`、置き場、tun のキー、見張り役の引き継ぎ、resolver の待ち受け）をそれぞれのステップで先に確かめてから最後にまとめる。

テストは実装ではなく仕様を確かめる。例の Given、When、Then と要求の本文から条件と期待を書き出してからテストを書き、仕様に無い細部（説明文の言い回し、内部名、引数の細かい並び）は固定しない。診断は種類と終了コードと標準出力が空であることで確かめる（`docs/ir/core/core-process.md#REQ-288`）。テストには、そのテストが実際に確かめる項目の印だけを付ける。レビューで確かめる項目に変わった REQ-286 と EX-522 の印は、今のテストから外す。

## Scope of change

- `src/cli.rs`、`src/startup.rs`、`src/main.rs`、`src/plan_text.rs`、`src/plan_json.rs`
- `crates/kakoi-core/src/`
  - `plan.rs`、`planning.rs`、`launch.rs`、`placement.rs`、`policy.rs`、`layers.rs`、`isolated_env.rs`、`guard_placement.rs`、`lib.rs`
  - 置き場の事実と作成を扱う新しいモジュール（名前は実装者が決める）
- `crates/kakoi-net/src/`
  - `application.rs`、`filtered.rs`、`namespace/dns.rs` と、resolver の待ち受けに要る範囲
- `tests/`
  - 入れ子、CLI、init、計画、配置、起動、ガードレール、ネットワークのテストと `tests/common`
- 公開文書と参照
  - `README.md`、`docs/cli.md`、`docs/policy.md`、`docs/security.md`、`docs/guide/`（ガイドの印が古くなった節だけ）、`CHANGELOG.md` の未リリースの節、ルートの `CONTEXT.md`、`PROJECT.md`
- シムの入れ子についてのコメント（`KAKOI=1` を見つけると書いている行だけ）: `examples/shim/codex` と `skills/kakoi-setup/assets/shim/codex`（2 つは同じ内容でなければならない）

## Step order and prerequisites

- S1（入れ子の印と判定、`--nested=exec` の入れ子のテストの作り直し）が最初。ほかのすべてのステップの入れ子のテストがこれに依存する。
- S2（`--nested` の文法）は S1 と独立だが、S3 が使う。
- S3（`--nested=isolate` の起動と計画）は S1 と S2 の後。
- S4（共有ファイルの置き場）は S3 の後。EX-892 と「入れ子の起動は置き場に触らない」の確かめに `--nested=isolate` が要るため。S5（置き場の保護）は S4 の後。
- S6（tun のキー）は S3 の後。tun の無い状態を入れ子で作るため。
- S7（外の見張り役の引き継ぎ）は S3 の後。
- S8（resolver の待ち受け）は S1 の後ならいつでもよい。
- S9（入れ子の filtered の端から端まで）は S4、S6、S7、S8 の後。
- S10（公開文書）は S1〜S9 の後。文書が振る舞いと一致していることを最後にまとめて確かめるため。
- S11（計画の範囲の最終確認）は最後。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-455、REQ-284、REQ-285、REQ-261、REQ-262、REQ-263、REQ-290、REQ-293、REQ-401、REQ-309、REQ-314（入れ子と設定ディレクトリの部分）、REQ-315（印の引数）、REQ-259 | EX-884、EX-885、EX-499、EX-501、EX-502、EX-520、EX-521、EX-547 |
| S2 | REQ-464、REQ-252 | EX-896、EX-897 |
| S3 | REQ-456、REQ-457、REQ-297、REQ-299 | EX-886、EX-888 |
| S4 | REQ-460、REQ-461、REQ-305、REQ-306（置き場の部分）、REQ-308、REQ-314（置き場の部分）、REQ-287 | EX-892、EX-893、EX-894、EX-898 |
| S5 | REQ-462、REQ-295 | EX-895 |
| S6 | REQ-458、REQ-315（tun の引数） | EX-890 |
| S7 | REQ-465、REQ-466、REQ-315（見張り役の引数） | EX-899、EX-900、EX-901 |
| S8 | REQ-459 | なし（S9 の EX-891 で通信として確かめる） |
| S9 | REQ-456、REQ-458、REQ-459、REQ-460（filtered の resolv.conf）、REQ-455（filtered の印） | EX-887、EX-889、EX-891 |
| S10 | REQ-463、REQ-286、REQ-183、REQ-196、REQ-353、TBL-160、TBL-158（レビュー） | EX-522（レビュー） |
| S11 | この計画のすべての項目 | この計画のすべての例 |

## Left to the implementer

- 新しいモジュール、関数、型の名前と、置き場や入れ子の事実を運ぶ値の形
- 入れ子の印のファイルの中身（仕様は存在と読み取り専用であることだけを定める）
- 共有ファイルの置き場の中のファイル名と、作り直すときの一時的な名前（一時的な名前は、並列起動どうしでぶつからないよう起動ごとに一意にする）
- 計画の要約で tun を見せることを示す 1 行と、入れ子の表示の行の言い回し（計画の文字の形は契約ではない。JSON の鍵 `nested` と `applied` は契約）
- 入れ子のテストで外の kakoi に渡す policy ファイルの置き場所（テスト用の一時ディレクトリの中）

## Stop conditions

- 仕様の要求どうし、または要求と例が食い違い、どちらかに決めないと実装できない
- 入れ子の印を `/dev` の中に読み取り専用で置く方法、または外の見張り役の置き場を作り直した `/dev` に読み取り専用で見せる方法が、bwrap 0.9.0 で得られない
- 組み立てた kakoi の隔離の中で組み立てた kakoi をもう一度起動する形が、テストの環境（CI を含む）で userns の制限などにより作れない
- 置き場の実在のファイルを読み取り専用で bind しても、中の bwrap がその上にマウントできない（決定の記録の実測と食い違う）
- pasta で通信するテストの方式の中で、外と中の kakoi を重ねた filtered が作れない
- 既存のテストが、今回の仕様の変更と関係の無い理由で失敗する

## Test command

```sh
cargo test --workspace --all-targets --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

## Out of scope

- 外と中の policy で同じファイルを `rw-copy` にしたときに止まる件（決定の記録の U1。判断待ち）
- 外と中の両方の見張り役を重ねる形（U2。判断待ち。この版では REQ-466 のとおり止める）
- `rw-copy` のファイルの置き方の変更
- 3 段目の入れ子の隔離を動かすこと（止まることは既知の隙間として文書に書くだけ）
- 許可リスト方式（全部隠して必要なものだけ見せる形）

## Steps

### S1: 入れ子を隔離の中の印で判定し、入れ子のテストを本物の入れ子で作り直す

- Purpose: 隔離の中の `/dev/kakoi-isolated` に読み取り専用の印を置いて kakoi がその有無だけで入れ子を判定するようにし、`--nested=exec` の入れ子を確かめる既存のテストを本物の入れ子の形に作り直す
- Specification: docs/ir/core/core-nested-isolation.md#REQ-455, docs/ir/core/core-process.md#REQ-284, docs/ir/core/core-process.md#REQ-285, docs/ir/core/core-command-resolution.md#REQ-261, docs/ir/core/core-command-resolution.md#REQ-262, docs/ir/core/core-command-resolution.md#REQ-263, docs/ir/core/core-output.md#REQ-290, docs/ir/core/core-output.md#REQ-293, docs/ir/core/core-output.md#REQ-401, docs/ir/core/core-runtime.md#REQ-309, docs/ir/core/core-runtime.md#REQ-314, docs/ir/core/core-runtime.md#REQ-315, docs/ir/core/core-init.md#REQ-259
- Prerequisites: なし
- May change: src/startup.rs, src/main.rs, src/plan_text.rs, src/plan_json.rs, crates/kakoi-core/src/plan.rs, crates/kakoi-core/src/planning.rs, crates/kakoi-core/src/launch.rs, crates/kakoi-core/src/isolated_env.rs, tests/
- Done when: EX-884、EX-885、EX-499、EX-501、EX-502、EX-520、EX-521、EX-547 が本物の入れ子（組み立てた kakoi の隔離の中で組み立てた kakoi を起動する形）で通り、REQ-261、REQ-262、REQ-263、REQ-284、REQ-285、REQ-290、REQ-293、REQ-309、REQ-401 に印の付いたテストが本物の入れ子で新しい本文どおりに通り、REQ-314 のテストが「ホストで KAKOI=1 を立てても入れ子にならない」ことと設定ディレクトリの選び方を確かめ、host と none の隔離の中に `/dev/kakoi-isolated` が通常ファイルとしてあって中から書くことも rm・mv で消すこともできず、固定引数の `--dev /dev` の直後に印が置かれることが計画の JSON の bwrap 引数で確かめられ、今のテストから REQ-286 と EX-522 の印が外れていて、隔離の中で `env -u KAKOI` を使う既存のテスト 2 本が EX-884 の形に書き換えられている（`--nested=isolate` の版は S3 で作る）
- Shown by: test — EX-884、EX-885、EX-499、EX-501、EX-502、EX-520、EX-521、EX-547、REQ-455 の印の読み取り専用、REQ-315 の印の位置と、上に挙げた既存の要求の作り直したテスト
- Left to the implementer: 印の有無を読む場所（`src/startup.rs` か事実を読む既存のモジュールか）と、それを計画の組み立てに渡す形。REQ-309 の「上げない」を本物の入れ子で観測する方法（外の隔離の中のシェルで `ulimit -Sn` を下げてから中の kakoi を起動する、など）
- Stop and hand back if: 既存のテストのうち、ホストで KAKOI=1 を立てることでしか作れない前提を確かめているものがあり、本物の入れ子で同じ観測ができない

### S2: `--nested` の文法を受け付ける

- Purpose: `--nested=exec` と `--nested=isolate` を実行と計画表示の形で受け付け、ほかの形を usage にする
- Specification: docs/ir/core/core-cli-syntax.md#REQ-464, docs/ir/core/core-cli-syntax.md#REQ-252
- Prerequisites: なし
- May change: src/cli.rs, tests/cli.rs
- Done when: EX-896（`=isolate` だけを受け付け、分離形、未知の値、値の無い形は usage で 125）と EX-897（init に付けると usage で 125）が通り、`--nested` の重複が usage になり、`--version` と `--help` の単独の形に付けると usage になる
- Shown by: test — EX-896、EX-897、REQ-464 の重複と単独の形
- Left to the implementer: none
- Stop and hand back if: 既存の文法の解析が `=` の形だけを受け付けるオプションを `--print-plan` 以外に持てない作りで、大きな書き換えが要る

### S3: `--nested=isolate` の起動と計画

- Purpose: `--nested=isolate` の起動が入れ子でも入れ子でない起動と同じ手順で隔離を作り、計画に入れ子の表示と `applied` を出すようにする
- Specification: docs/ir/core/core-nested-isolation.md#REQ-456, docs/ir/core/core-nested-isolation.md#REQ-457, docs/ir/core/core-plan-output.md#REQ-297, docs/ir/core/core-plan-output.md#REQ-299
- Prerequisites: S1、S2
- May change: src/startup.rs, src/main.rs, src/plan_text.rs, src/plan_json.rs, crates/kakoi-core/src/planning.rs, crates/kakoi-core/src/plan.rs, tests/
- Done when: EX-886（入れ子の中で `--nested=isolate` を付けても外で rw でない場所には書けず、入れ子の警告が出ない）と EX-888（`nested` と `applied` の 3 通り）が通り、`--nested=isolate` の入れ子の起動がホストの PATH ではなく隔離の PATH でコマンドを解決し、HOME の検査を行い、外の隔離が秘密ファイルを空にしているときにコマンドを実行せず secret の診断で 125 になり、`--nested=isolate` の入れ子の計画の要約と全量の両方に入れ子の表示があって `--nested=exec` の入れ子の表示（使われない）と区別できることがテストで確かめられる
- Shown by: test — EX-886、EX-888、REQ-456 の PATH の解決・HOME の検査・秘密の場合、REQ-457 と REQ-297 の表示
- Left to the implementer: 計画の要約の入れ子の表示の言い回し
- Stop and hand back if: `--nested=isolate` の入れ子の起動が、入れ子でない起動と違う手順を要する箇所（ファイルの置き方と見張り役の引き継ぎ以外）が見つかる

### S4: 共有ファイルの置き場で hide の空ファイルを置く

- Purpose: 入れ子でない起動が、hide の空のファイルと filtered の resolv.conf を `$XDG_RUNTIME_DIR/kakoi/` の実在のファイルの読み取り専用の bind で置き、使えないときと入れ子の起動はデータから作るファイルで置き、それ以外にホストのファイルを書かないようにする
- Specification: docs/ir/core/core-nested-isolation.md#REQ-460, docs/ir/core/core-nested-isolation.md#REQ-461, docs/ir/core/core-runtime.md#REQ-305, docs/ir/core/core-runtime.md#REQ-306, docs/ir/core/core-runtime.md#REQ-308, docs/ir/core/core-runtime.md#REQ-314, docs/ir/core/core-process.md#REQ-287
- Prerequisites: S3
- May change: crates/kakoi-core/src/, crates/kakoi-net/src/application.rs, crates/kakoi-net/src/filtered.rs, src/main.rs, src/plan_text.rs, src/plan_json.rs, tests/
- Done when: EX-892、EX-893、EX-894、EX-898 が通り、次がテストで確かめられる。置き場のディレクトリが 0700、ファイルが 0600 で作られる。ファイルの権限違い（0644）が作り直される。ディレクトリがリンクか権限違いのとき、または `XDG_RUNTIME_DIR` が空か相対パスのとき、警告なしでデータの形に切り替わる。コマンドが見つからず 127 で終わる起動は置き場を作らない。計画表示は置き場に書き込まず、入れ子でなく `XDG_RUNTIME_DIR` が絶対パスなら置き場の形を、そうでなければデータの形を示す。`--nested=isolate` の入れ子の起動は置き場に触れない。コマンドを包む起動（計画表示を含む）は、置き場のほかにホストのファイルを書かない
- Shown by: test — EX-892、EX-893、EX-894、EX-898、REQ-461 の作成・権限・切り替え・127 の場合、REQ-460 の計画表示と入れ子の起動、REQ-305 の置き場以外に書かないこと
- Left to the implementer: 置き場の事実を読むモジュールの形と、起動の直前に引数を差し替える方法
- Stop and hand back if: 読み取り専用で bind した置き場のファイルの上に、中の bwrap がマウントできない

### S5: 共有ファイルの置き場を書き込める項目から守る

- Purpose: 置き場を使う起動で、置き場を保護対象に加え、書き込める項目が置き場を含むか置き場の中にあるときに path の診断で止める
- Specification: docs/ir/core/core-nested-isolation.md#REQ-462, docs/ir/core/core-plan-output.md#REQ-295
- Prerequisites: S4
- May change: crates/kakoi-core/src/placement.rs, crates/kakoi-core/src/planning.rs, crates/kakoi-core/src/plan.rs, S4 で作った置き場のモジュール, tests/
- Done when: EX-895（置き場を rw にすると path の診断で 125）が通り、置き場の中のファイルを rw-file にする形も同じ診断で止まり、計画表示でも同じ診断で止まり、ほかの保護対象と同時に当たるときは置き場が最後に名指されることがテストで確かめられる
- Shown by: test — EX-895、REQ-462 の置き場の中の rw-file と計画表示、REQ-295 の順
- Left to the implementer: none
- Stop and hand back if: 置き場を使えない起動（`XDG_RUNTIME_DIR` が無いなど）でこの検査を当てるかどうかについて、REQ-462 の「置き場を使う起動では」の読み方が既存の配置の検査と食い違う

### S6: network.allow-nested-filtered で tun を見せる

- Purpose: policy のキー `network.allow-nested-filtered` を読み、true のとき隔離の中にホストの `/dev/net/tun` を見せ、ホストに無ければ bwrap の診断で止める
- Specification: docs/ir/core/core-nested-isolation.md#REQ-458, docs/ir/core/core-runtime.md#REQ-315
- Prerequisites: S3
- May change: crates/kakoi-core/src/policy.rs, crates/kakoi-core/src/layers.rs, crates/kakoi-core/src/plan.rs, crates/kakoi-core/src/planning.rs, src/plan_text.rs, src/plan_json.rs, tests/
- Done when: EX-890（キーが無ければ隔離の中に `/dev/net/tun` が無い）が通り、次がテストで確かめられる。true のとき隔離の中に `/dev/net/tun` がある。段をまたぐと上の段の値が勝つ。host のポリシーでこのキーだけを true にしても警告も診断も出ない。tun の無い環境で true にすると、実行も計画表示も bwrap の診断で 125 になる。tun の無い環境は、tun を見せていない外の隔離の中で `--nested=isolate` の起動に true を持たせて作る。計画の要約にそのことを示す 1 行がある。引数が印の直後に置かれる
- Shown by: test — EX-890、REQ-458 の見せる場合・段・警告なし・tun が無い場合・要約、REQ-315 の tun の位置
- Left to the implementer: none
- Stop and hand back if: `network.allow-nested-filtered` を「残る通信許可・公開設定」に数えないと、既存のネットワークの要求（`docs/ir/network/policy/network-mode.md`）と食い違う振る舞いが要る

### S7: 外の見張り役を入れ子の中の隔離に引き継ぐ

- Purpose: `--nested=isolate` の起動が `/dev/kakoi-guard` を中の隔離に読み取り専用で引き継ぎ、中のポリシーにも見張り役があるときは policy の診断で止める
- Specification: docs/ir/core/core-nested-isolation.md#REQ-465, docs/ir/core/core-nested-isolation.md#REQ-466, docs/ir/core/core-runtime.md#REQ-315
- Prerequisites: S3
- May change: crates/kakoi-core/src/plan.rs, crates/kakoi-core/src/planning.rs, crates/kakoi-core/src/guard_placement.rs, src/startup.rs, tests/
- Done when: EX-899（外の見張り役の規則が中の隔離でも効いて 126）、EX-900（本物の場所に重ねた見張り役が中の隔離でも本物を起動する）、EX-901（外と中の両方に見張り役があると policy の診断で 125）が通り、外に見張り役が無いときは何も引き継がず、引き継ぐ引数が印と tun の後に置かれることがテストで確かめられる
- Shown by: test — EX-899、EX-900、EX-901、REQ-465 の外に見張り役が無い場合、REQ-315 の見張り役の引数の位置
- Left to the implementer: none
- Stop and hand back if: 引き継いだ見張り役が、中の隔離の中で外の隔離と違う振る舞いをする（本物の置き直し先を見失う、表を読めない）

### S8: filtered の resolver が 127.0.0.54 でも待ち受ける

- Purpose: filtered の隔離の中の resolver を 127.0.0.53 と 127.0.0.54 の両方のポート 53 で待ち受けさせ、resolv.conf の中身は変えない
- Specification: docs/ir/core/core-nested-isolation.md#REQ-459
- Prerequisites: S1
- May change: crates/kakoi-net/src/, crates/kakoi-core/src/network.rs, tests/
- Done when: filtered の隔離の中から 127.0.0.54 のポート 53 に DNS の問い合わせを送ると、127.0.0.53 に送ったときと同じく許可規則どおりの答えが返り、隔離の中の resolv.conf が `nameserver 127.0.0.53` の 1 行のままであることがテストで確かめられる
- Shown by: test — REQ-459 の 2 つの宛先と resolv.conf の中身（pasta で通信する既存のテストの方式）
- Left to the implementer: 2 つの宛先を 1 つの受け口で扱うか別々に扱うか
- Stop and hand back if: 127.0.0.54 で待ち受けると、既存の DNS の要求（`docs/ir/network/` の DNS の境界や上流の扱い）と食い違う振る舞いが要る

### S9: 入れ子の filtered を端から端まで確かめる

- Purpose: 外が host と filtered のそれぞれで、`network.allow-nested-filtered` を許した外の中で `--nested=isolate` の filtered が許可先だけに通信でき、許していない外の中では実行せずに止まることを確かめる
- Specification: docs/ir/core/core-nested-isolation.md#REQ-455, docs/ir/core/core-nested-isolation.md#REQ-456, docs/ir/core/core-nested-isolation.md#REQ-458, docs/ir/core/core-nested-isolation.md#REQ-459, docs/ir/core/core-nested-isolation.md#REQ-460
- Prerequisites: S4、S6、S7、S8
- May change: tests/
- Done when: EX-887（外が tun を見せていない中で、pasta を使える状態で `--nested=isolate` の filtered を頼むと、コマンドを実行せずに診断で終わる。同じ構成で外が tun を許せば EX-889 のとおり動く）、EX-889、EX-891 が通り、filtered の隔離の中にも `/dev/kakoi-isolated` があって中から書けないことと、置き場を使えた filtered の隔離の中の resolv.conf が 0600 の `nameserver 127.0.0.53` の 1 行に見えることがテストで確かめられる
- Shown by: test — EX-887、EX-889、EX-891、REQ-455 の filtered の印、REQ-460 の filtered の resolv.conf（pasta で通信する既存のテストの方式で、外と中の kakoi を重ねる）
- Left to the implementer: 許可先と許可外の宛先の選び方（既存の pasta のテストが使う代わりの宛先の方式に合わせる）
- Stop and hand back if: 既存の pasta のテストの方式の中で外と中の kakoi を重ねた filtered が作れない、または通信の結果が決定の記録の実測（外に tun を見せれば入れ子の filtered が動いた）と食い違う

### S10: 公開文書と参照を振る舞いに合わせる

- Purpose: README、docs/cli.md、docs/policy.md、docs/security.md、ガイド、変更履歴、ルートの用語集、PROJECT.md、シムのコメントを、入れ子の判定と `--nested`、`network.allow-nested-filtered`、置き場の例外、既知の隙間の変更に合わせる
- Specification: docs/ir/core/core-nested-isolation.md#REQ-463, docs/ir/core/core-process.md#REQ-286, docs/ir/core/core-terms.md#REQ-183, docs/ir/core/core-terms.md#REQ-196, docs/ir/core/core-product.md#REQ-353, docs/ir/core/core-runtime.md#REQ-305, docs/ir/core/core-runtime.md#REQ-314
- Prerequisites: S1〜S9
- May change: README.md, docs/cli.md, docs/policy.md, docs/security.md, docs/guide/, CHANGELOG.md, CONTEXT.md, PROJECT.md, examples/shim/codex, skills/kakoi-setup/assets/shim/codex
- Done when: REQ-463、REQ-286、REQ-183、REQ-196、REQ-353 の how_to_verify の手順どおりに読んで各事項が書かれていて、docs/security.md の既知の隙間が TBL-160 の行 8 と 17 と一致し、README が信頼する起動環境（REQ-314）を新しい本文どおりに書き、ルートの CONTEXT.md の「入れ子」が印による判定で定義され、PROJECT.md の「コマンドを包む起動はファイルを書かない」の約束に置き場の例外（REQ-305）が入り、PROJECT.md のテスト環境の段落が本物の入れ子のテストと tun の無い環境の作り方を書き、2 つのシムの写しが同じ内容で、`kotowari check` に guide_stale の通知が残らず、CHANGELOG.md の未リリースの節に利用者から見える変更（`--nested`、入れ子の判定の変更、`network.allow-nested-filtered`、hide と resolv.conf の置き方、外の見張り役の引き継ぎ）が書かれている
- Shown by: check — 各 how_to_verify の手順で文書を読む、`cmp examples/shim/codex skills/kakoi-setup/assets/shim/codex` が差分なし、`kotowari check --format json | jq -r '.findings[] | select(.kind == "guide_stale") | .detail'` が空
- Left to the implementer: 文書の中の言い回しと節の組み立て
- Stop and hand back if: 文書に書くべきことが要求の本文から一意に決まらない

### S11: 計画の範囲が揃ったことを確かめる

- Purpose: この計画の要求と例のすべてに印の付いたテストがあり、変更したファイルとこの計画の ID に kotowari の誤りが無いことを確かめる
- Specification: docs/ir/core/core-nested-isolation.md#REQ-455, docs/ir/core/core-cli-syntax.md#REQ-464
- Prerequisites: S1〜S10
- May change: なし（見つかった漏れは該当するステップの範囲で直す）
- Done when: Verification map のレビュー以外の要求と例のそれぞれで `kotowari query ID` の `tests` が空でなく、`kotowari check --format json` にこのブランチが変えたファイルとこの計画の ID についての誤りが無く、Test command がすべて通る
- Shown by: check — `for id in REQ-455 REQ-456 REQ-457 REQ-458 REQ-459 REQ-460 REQ-461 REQ-462 REQ-464 REQ-465 REQ-466 REQ-284 REQ-285 REQ-287 REQ-261 REQ-262 REQ-263 REQ-290 REQ-293 REQ-401 REQ-305 REQ-306 REQ-308 REQ-309 REQ-314 REQ-315 REQ-295 REQ-297 REQ-299 REQ-252 REQ-259 EX-884 EX-885 EX-886 EX-887 EX-888 EX-889 EX-890 EX-891 EX-892 EX-893 EX-894 EX-895 EX-896 EX-897 EX-898 EX-899 EX-900 EX-901 EX-499 EX-501 EX-502 EX-520 EX-521 EX-547; do kotowari query $id | jq -e '.items[0].tests != []' >/dev/null || echo "missing $id"; done` の出力が空、続けて Test command
- Left to the implementer: none
- Stop and hand back if: この計画の外の既存の指摘が、この計画の変更で増えている
