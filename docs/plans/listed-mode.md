# Plan: 見せるものを選ぶマウントとコマンドのモード "listed" を作る

## Goal

利用者がプロファイルに `mounts.mode = "listed"` と書くと、土台と書いた場所だけが隔離の中に見えてほかの場所（ソケットや認証情報の置き場）が存在しなくなり、`commands.mode = "listed"` と `commands.allow` で起動できるプログラムを絞れ、ガードレールの規則の `only` で書いた使い方以外を止められ、`kakoi init NAME --example listed` でその見本を書き出せる状態にする。

## Specification

IR は `docs/ir/`。決定の記録は `docs/decision/brainstorm/2026-09-30-allowlist.md`（A1〜A58。A2、A18、A27、A28、A29 は置き換え済み。U1 は保留）。要求の本文は `kotowari query REQ-nnn`、例は `kotowari query EX-nnn` で読む。本文をこの計画に写さないので、各ステップの前に必ず読むこと。用語「土台」「見せた場所」「隔離の中の最初のプロセス」は `docs/ir/core/CONTEXT.md` にある。

- 新しい要求
  - マウント: `docs/ir/core/core-listed-mounts.md#REQ-467`、`#REQ-468`、`#REQ-469`、`#REQ-470`、`#REQ-471`、`#REQ-472`、`#REQ-473`、`#REQ-483`、`#REQ-484`
  - コマンド: `docs/ir/core/core-listed-commands.md#REQ-474`、`#REQ-475`、`#REQ-476`、`#REQ-477`、`#REQ-478`、`#REQ-485`
  - ガードレール: `docs/ir/core/core-command-guard-rules.md#REQ-480`
  - init: `docs/ir/core/core-init.md#REQ-482`
- 新しい例: EX-902〜EX-934、EX-937〜EX-939、EX-942〜EX-954
- 今回文を改めた既存の要求と表（今あるテストは古い本文を確かめているので、新しい本文に合わせて見直す）: `docs/ir/core/core-mounts.md#REQ-167`、`docs/ir/core/core-policy.md#REQ-151`、`docs/ir/core/core-policy.md` の TBL-152、`docs/ir/core/core-command-guard-rules.md#REQ-438`、`#REQ-443`、`docs/ir/core/core-command-guard-runtime.md#REQ-447`、`docs/ir/core/core-init.md#REQ-256`、`#REQ-259`
- レビューで確かめる要求と表: `docs/ir/core/core-listed-commands.md#REQ-479`、`docs/ir/core/core-bundled-profile.md#REQ-481`、`docs/ir/core/core-public-docs.md#REQ-359`、`docs/ir/core/core-public-docs.md` の TBL-160（行 18 と 19）。EX-935、EX-936、EX-940、EX-941、EX-658 はレビューの要求だけを指すので、テストではなくレビューで確かめる

## Approach and why

ポリシーのキーと合成を最初に作る。マウントの見え方、コマンドの絞り、計画表示のどれも「合成後のモード」に依存するからである。二つのモードは、どれかの段が "listed" なら上の段で戻らない（TBL-152 の新しい行）。今の `network.mode` や `env.mode` の「上の段が上書き」とは規則が違うので、`crates/kakoi-core/src/layers.rs` のスカラーの合成に混ぜず、厳しい側が残る合成として分けて書く。

"listed" のマウントは、`crates/kakoi-core/src/plan.rs` の `bwrap_arguments` の先頭の固定引数 `--ro-bind / /` を差し替えて作る。根を空の tmpfs にし、土台、項目、kakoi が自分で置くもの、項目のパスの途中のリンク（`--symlink` で作り直す）、見せた場所の祖先を並べ、最後に根を読み取り専用にする。祖先は bwrap に黙って作らせず（所有者だけが読める権限になる。`guard_arguments` のコメントにある実測）、計画の側で明示して作る。どの場所を見せたかの判断（見せた場所の外に向いた hide、走査、hide-mounts を飛ばす、作業ディレクトリの検査、見張り役の本物の選び方）は、ホストに触れない計画の組み立ての中で、既存の事実（`mount_facts.rs` などが読んだもの）から行う。今の `is_nested` と同じく、ホストを読む処理は外側に置き、値として渡す形を保つ。

Landlock はカーネルのシステムコール（`landlock_create_ruleset`、`landlock_add_rule`、`landlock_restrict_self`）を `libc` で直接呼ぶ。seccomp のフィルタを `crates/kakoi-core/src/seccomp.rs` で自前に組んでいるのと同じ扱いで、新しい依存を足さない。Landlock はマウントを禁じる（決定の記録の実測）ので、実行の制限は bwrap がマウントを終えた後、隔離の中でしか掛けられない。そのため、コマンドのモードが "listed" のときは、bwrap にコマンドの代わりに kakoi 自身を「隔離の中の最初のプロセス」として起動させる。見分け方は、ガードレールの見張り役（`src/guard.rs` の `run_if_guard`）と filtered の最初のプロセス（`crates/kakoi-net/src/init.rs` の `run_if_requested`）と同じく起動の名前で行い、`src/main.rs` で入れ子の判定より前に置く。kakoi 自身の実行ファイルは、見張り役と同じ置き方（`GUARD_ROOT` の tmpfs の中）で隔離に置けば、マウントが "listed" で kakoi の場所を見せていなくても起動できる。filtered では隔離のプロセス 1 がすでに kakoi-net-init なので、その下で最初のプロセスの役を挟んでからコマンドを起動する形になる。抽象 UNIX ソケットの scope はマウントを禁じない（実測で bwrap が動いた）ので、bwrap を起動する前に kakoi 自身に掛ける。最初のプロセスで掛けると、コマンドのモードが "host" の起動にも最初のプロセスを挟むことになり、コマンドの起動の失敗の扱い（REQ-262）が変わってしまう。

Landlock を使えるかと scope を使えるかの確かめは、`/dev/net/tun` の確かめ（`crates/kakoi-core/src/planning.rs` の `allow_nested_filtered` のところ）と同じ段階で行う。計画表示でも行う。

`only` は `crates/kakoi-core/src/guard.rs` の今の照合（REQ-439 の語の照合、REQ-440 の読み飛ばし）をそのまま使い、照合の順（REQ-443）の最後に足す。見張り役の 1 行の形（REQ-447）に `only` の場合を足す。

テストは実装ではなく仕様を確かめる。例の Given、When、Then と要求の本文から条件と期待を書き出してからテストを書き、仕様に無い細部（説明文の言い回し、内部名、引数の細かい並び）は固定しない。診断は種類と終了コードと標準出力が空であることで確かめる（`docs/ir/core/core-process.md#REQ-288`）。テストには、そのテストが実際に確かめる項目の印だけを付ける。Landlock と scope を確かめるテストは、使えない機械では飛ばさずに失敗させる（決定の記録の A39）。CI の ubuntu-24.04 のカーネルは 6.17（決定の記録の A39 の出典）で、そこで Landlock が有効かは S5 で確かめる。

## Scope of change

- `src/`
  - `main.rs`、`startup.rs`、`cli.rs`、`init.rs`、`guard.rs`、`plan_text.rs`、`plan_json.rs`
  - 隔離の中の最初のプロセスの役を扱う新しいモジュール（名前は実装者が決める）
- `crates/kakoi-core/src/`
  - `policy.rs`、`layers.rs`、`mounts.rs`、`mount_facts.rs`、`plan.rs`、`planning.rs`、`placement.rs`、`guard.rs`、`guard_placement.rs`、`launch.rs`、`workspace_facts.rs`、`lib.rs`
  - Landlock を扱う新しいモジュール（名前は実装者が決める）
- `crates/kakoi-net/src/`
  - `application.rs`、`supervisor.rs`、`init.rs`（filtered で最初のプロセスの役を挟む範囲だけ）
- `examples/profile/listed.toml`（新規）
- `tests/`
  - `policy.rs`、`layers.rs`、`mounts.rs`、`launch.rs`、`plan.rs`、`guard.rs`、`cli.rs`、`bundled_profile.rs`、`nested.rs`、`tests/common`、`tests/kakoi_net/` の filtered の起動を確かめる範囲
  - "listed" を確かめる新しいテストファイル（名前は実装者が決める）
- 公開文書と参照
  - `README.md`、`docs/cli.md`、`docs/policy.md`、`docs/security.md`、`docs/guide/`、`CHANGELOG.md` の未リリースの節、`PROJECT.md`

## Step order and prerequisites

- S1（ポリシーのキーと合成）が最初。ほかのすべてのステップが合成後のモードを使う。
- S2（"listed" の見え方）は S1 の後。S3（見せた場所の外の扱い、作業ディレクトリ、リンク）は S2 の後。
- S4（Landlock の土台と抽象 UNIX ソケットの scope）は S2 の後。scope は "listed" のマウントでだけ掛けるため。
- S5（コマンドの "listed"）は S1 と S4 の後。
- S6（計画表示）は S3 と S5 の後。飛ばした項目と許した項目を表示に出すため。
- S7（ガードレールの `only`）は S1 の後。S1 と同じ `crates/kakoi-core/src/guard.rs` の型を変えるため。S8 が使う。
- S8（init の `--example` と見本）は S3、S5、S7 の後。見本が "listed" と `only` を使い、読み込めることを確かめるため。
- S9（公開文書とガイド）は S1〜S8 の後。
- S10（計画の範囲の最終確認）は最後。

## Verification map

| Step | Requirements | Examples |
|---|---|---|
| S1 | REQ-467、REQ-474、REQ-151、TBL-152 | EX-902、EX-903、EX-904、EX-920、EX-921、EX-946 |
| S2 | REQ-468（見え方、土台、飛ばす土台）、REQ-469、REQ-470、REQ-471、REQ-167 | EX-905、EX-906、EX-907、EX-908、EX-909、EX-910、EX-911、EX-912、EX-913、EX-914、EX-915 |
| S3 | REQ-468（見せた場所の外、本物、リンク）、REQ-483、REQ-484 | EX-942、EX-943、EX-944、EX-945、EX-949、EX-950、EX-951 |
| S4 | REQ-472 | EX-916、EX-917 |
| S5 | REQ-475、REQ-476、REQ-477、REQ-485 | EX-922、EX-923、EX-924、EX-925、EX-926、EX-927、EX-928、EX-947、EX-948、EX-952、EX-953、EX-954 |
| S6 | REQ-473、REQ-478 | EX-918、EX-919、EX-929、EX-930 |
| S7 | REQ-438、REQ-443、REQ-480、REQ-447 | EX-931、EX-932、EX-933、EX-934 |
| S8 | REQ-482、REQ-256、REQ-259、REQ-481（レビュー） | EX-937、EX-938、EX-939。EX-935 と EX-936 は計画の完了後の受け入れで人が確かめる |
| S9 | REQ-479、REQ-359、TBL-160（レビュー） | EX-940、EX-941、EX-658（レビュー） |
| S10 | この計画のすべての項目 | この計画のすべての例 |

## Left to the implementer

- 新しいモジュール、関数、型の名前と、合成後のモードや見せた場所を運ぶ値の形
- 隔離の中の最初のプロセスを見分ける起動の名前と、kakoi 自身の実行ファイルを置くパス（見張り役の tmpfs の中であること、既存の見張り役と名前がぶつからないこと）
- 最初のプロセスが Landlock の規則を受け取る方法（引数、環境変数、読み取り専用のファイルのどれか。隔離の中のコマンドから書き換えられないこと）
- 根を読み取り専用にする bwrap の引数の組み方（`--remount-ro /` など）と、祖先のディレクトリの権限（0755 を目安にする）
- 計画の要約の 2 行の言い回し（計画の文字の形は契約ではない。JSON の鍵の名前は実装者が決め、S6 で公開文書に書く）
- テストで "listed" を確かめるときのプロファイルの置き場所。ただし HOME、ワークスペース、プロファイルは "/tmp" の外（`tests/common` の `TempDir::under` で `CARGO_TARGET_TMPDIR` の下など）に置く。"listed" では "/tmp" が隔離専用の書ける場所になるので、"/tmp" の下に置くと EX-910 と EX-912 の前提が崩れる。EX-913 はホストの本物の "/tmp/kakoi" を使わず、テストごとに一意な "/tmp" の下の名前を "rw" に書いて確かめる

## Stop conditions

- 仕様の要求どうし、または要求と例が食い違い、どちらかに決めないと実装できない
- bwrap 0.9.0 で、空の tmpfs を根にして土台と項目を重ね、最後に根を読み取り専用にする形が作れない
- 項目のパスの途中のリンクを、同じ行き先を指す読み取り専用のリンクとして作り直せない
- 隔離の中の最初のプロセスが、Landlock を掛けてからコマンドを起動する形を、host、none、filtered のどれかで作れない
- CI の機械で Landlock（ABI 6 の scope を含む）が使えず、テストが失敗する
- 既存のテストが、今回の仕様の変更と関係の無い理由で失敗する

## Test command

```sh
cargo test --workspace --all-targets --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

## Out of scope

- EX-935 と EX-936 の人の確かめ（計画の完了後の受け入れで、利用者が WSL2 の上で `kakoi init work --example listed` で書き出したプロファイルで claude と codex を起動し、隔離の中で "~/.ssh" と "/run" が存在せず、`ld.so /usr/bin/git` を名前で起動すると止まることを確かめる）

- インタプリタを許さない使い方のためにさらに締める段（決定の記録の U1。許していないプログラムのファイルを見せない、書ける場所に noexec を付ける）
- 同梱の既定のプロファイル `examples/profile/default.toml` をどちらのモードにするかの変更（今のまま "host"）
- インタプリタに実行の可否を確かめさせる仕組み（決定の記録の A24 で退けた）
- セットアップスキル（`skills/kakoi-setup/`）の見本の選び方

## Steps

### S1: 二つのモードのキーと、厳しい側が残る合成を読み込む

- Purpose: ポリシーの `mounts.mode`、`mounts.system`、`commands.mode`、`commands.allow` を読み込み、どれかの段が "listed"（`mounts.system` は false）なら上の段で戻らない合成と、形の誤りをポリシー読み込み失敗にする検査を作る
- Specification: docs/ir/core/core-listed-mounts.md#REQ-467, docs/ir/core/core-listed-commands.md#REQ-474, docs/ir/core/core-policy.md#REQ-151
- Prerequisites: なし
- May change: crates/kakoi-core/src/policy.rs, crates/kakoi-core/src/layers.rs, crates/kakoi-core/src/guard.rs（ポリシーの commands の表を読む型だけ）, crates/kakoi-core/src/lib.rs, tests/policy.rs, tests/layers.rs
- Done when: EX-902、EX-903、EX-904、EX-920、EX-921、EX-946 がテストで通り（「計画を表示する」の例はこのステップでは合成の結果で確かめ、S6 で計画の表示から確かめ直す）、TBL-152 の新しい行（厳しい側が残る三つのキーと、上の段が足す `commands.allow`）が合成のテストで確かめられ、REQ-151 の既存のテストが新しい固定キーの一覧で通る
- Shown by: test — EX-902、EX-903、EX-904、EX-920、EX-921、EX-946 と、TBL-152 の行ごとの合成のテスト、REQ-151 の既存のテスト
- Left to the implementer: 合成後の値の型（列挙か真偽値か）と、合成の関数の分け方
- Stop and hand back if: 既存の合成の仕組みが、段ごとに厳しい側を残す形を入れられない作りで、仕様の変更なしに足せない

### S2: "listed" で土台と項目だけを見せる隔離を作る

- Purpose: マウントのモードが "listed" のとき、空の根に土台、項目、祖先、隔離専用の `/tmp`、名前解決の設定の指す先を並べ、根と祖先を読み取り専用にする bwrap の引数を組み、"host" の見え方の要求を "host" のときだけに当てる
- Specification: docs/ir/core/core-listed-mounts.md#REQ-468, docs/ir/core/core-listed-mounts.md#REQ-469, docs/ir/core/core-listed-mounts.md#REQ-470, docs/ir/core/core-listed-mounts.md#REQ-471, docs/ir/core/core-mounts.md#REQ-167
- Prerequisites: S1
- May change: crates/kakoi-core/src/plan.rs, crates/kakoi-core/src/planning.rs, crates/kakoi-core/src/mounts.rs, crates/kakoi-core/src/mount_facts.rs, crates/kakoi-core/src/lib.rs, src/startup.rs, src/main.rs, src/plan_text.rs と src/plan_json.rs（飛ばした土台の理由を出す範囲だけ）, tests/launch.rs, tests/mounts.rs, tests/plan.rs, tests/common, "listed" の新しいテストファイル
- Done when: EX-905〜EX-915 が組み立てた kakoi の実際の起動で通り（EX-908 は計画の JSON の bwrap 引数で、EX-909、EX-914、EX-915 は前提をホストで作れないので、計画の組み立てに渡す事実を差し替えて確かめてよい）、"host" のときの今のマウントのテストがすべて変わらず通り、REQ-167 の今のテストが "host" のときの決まりとして通る
- Shown by: test — EX-905、EX-906、EX-907、EX-908、EX-909、EX-910、EX-911、EX-912、EX-913、EX-914、EX-915 と REQ-167 の既存のテスト
- Left to the implementer: 見せた場所の一覧を作る場所（計画の組み立ての中か、その直前か）と、祖先を作る引数の並べ方
- Stop and hand back if: `/etc/resolv.conf` の指す先だけを見せると、そのファイルを含むディレクトリのほかの中身が見えてしまい、bwrap の引数で防げない

### S3: 見せた場所の外に向いたものを飛ばし、作業ディレクトリとリンクを扱う

- Purpose: "listed" のとき、見せた場所の外に向いた hide（秘密や走査から生成したものを含む）、走査、hide-mounts を理由付きで飛ばし、見張り役の本物と起動するコマンドの探索を隔離の中で見えているものに限り、作業ディレクトリが見せた場所の外なら止め、項目のパスの途中のリンクを作り直し、filtered の差し替えた "/etc/resolv.conf" を "mounts.system" によらず隔離の中の "/etc/resolv.conf" で読めるようにする
- Specification: docs/ir/core/core-listed-mounts.md#REQ-468, docs/ir/core/core-listed-mounts.md#REQ-483, docs/ir/core/core-listed-mounts.md#REQ-484
- Prerequisites: S2
- May change: crates/kakoi-core/src/mounts.rs, crates/kakoi-core/src/placement.rs, crates/kakoi-core/src/planning.rs, crates/kakoi-core/src/plan.rs, crates/kakoi-core/src/guard_placement.rs, crates/kakoi-core/src/workspace_facts.rs, crates/kakoi-core/src/mount_facts.rs, crates/kakoi-net/src/application.rs（差し替えた "/etc/resolv.conf" の置き先の範囲だけ）, src/plan_text.rs, src/plan_json.rs, tests/mounts.rs, tests/guard.rs, tests/launch.rs, tests/placement.rs, tests/kakoi_net/, "listed" の新しいテストファイル
- Done when: EX-942、EX-943、EX-944、EX-945、EX-949、EX-950、EX-951 がテストで通り、filtered で "mounts.system" が true と false のそれぞれで隔離の中の "/etc/resolv.conf" が kakoi の差し替えた中身で読め、その親のディレクトリが計画の側で作られ、作業ディレクトリの検査が計画表示でも種類 "path" の診断で終わり、"host" のときの今の hide、走査、hide-mounts、見張り役、作業場所のテストがすべて変わらず通る
- Shown by: test — EX-942、EX-943、EX-944、EX-945、EX-949、EX-950、EX-951
- Left to the implementer: 飛ばした理由の文言（計画の文字の形は契約ではない）と、作業ディレクトリの検査を置く段階の中の位置（REQ-294 の作業場所の検査の中であること）
- Stop and hand back if: filtered の差し替えた "/etc/resolv.conf" を、"listed" の根の下の "/etc/resolv.conf" に bwrap 0.9.0 で置けない。生成された hide の中に、見せた場所の外に向いていても飛ばすと守りが弱まるもの（見せた場所の中から別の経路で届くもの）が見つかる

### S4: Landlock の土台を作り、"listed" と host で抽象 UNIX ソケットを止める

- Purpose: Landlock のシステムコールを扱う部品と、使えるか（ABI と scope）を確かめる処理を作り、マウントのモードが "listed" でネットワークのモードが "host" のとき、隔離の外で作られた抽象 UNIX ソケットへの接続を止め、scope が無いホストでは bwrap の所在の段階と計画表示で止める
- Specification: docs/ir/core/core-listed-mounts.md#REQ-472
- Prerequisites: S2
- May change: crates/kakoi-core/src/planning.rs, crates/kakoi-core/src/launch.rs, crates/kakoi-core/src/lib.rs, Landlock の新しいモジュール, src/main.rs, src/startup.rs, tests/launch.rs, "listed" の新しいテストファイル
- Done when: EX-916 が、テストの中でホスト側に待ち受けた抽象 UNIX ソケットへの接続が隔離の中から拒否されることで通り、EX-917 が、scope が無いことを装う方法（計画に渡す事実を差し替える）で種類 bwrap の 125 になることで通り、scope が使えないことにした事実を渡しても、"listed" でネットワークのモードが "none" と filtered の起動は 125 にならずにコマンドを実行することがテストで確かめられる
- Shown by: test — EX-916、EX-917 と、"none" と filtered では scope を求めないことのテスト
- Left to the implementer: 使えるかを確かめる事実を計画に渡す形
- Stop and hand back if: CI の機械で Landlock か scope が使えない。scope を掛けると、隔離の中で作った抽象 UNIX ソケット（隔離の中のプロセスどうし）にも繋がらなくなる

### S5: コマンドの "listed" で、許していないプログラムの起動を止める

- Purpose: コマンドのモードが "listed" のとき、bwrap に隔離の中の最初のプロセスとして kakoi 自身を起動させ、それが `commands.allow` と自動で許すもの（動的リンカの実体、kakoi 自身）だけに実行を許す Landlock を掛けてからコマンドを起動し、起動の失敗を 126 の診断にし、無い項目と見えない項目を飛ばし、Landlock が使えないホストでは止める
- Specification: docs/ir/core/core-listed-commands.md#REQ-475, docs/ir/core/core-listed-commands.md#REQ-476, docs/ir/core/core-listed-commands.md#REQ-477, docs/ir/core/core-listed-commands.md#REQ-485
- Prerequisites: S1, S4
- May change: src/main.rs, src/startup.rs, src/plan_text.rs と src/plan_json.rs（飛ばした項目の理由を出す範囲だけ）, 最初のプロセスの新しいモジュール, crates/kakoi-core/src/plan.rs, crates/kakoi-core/src/planning.rs, crates/kakoi-core/src/launch.rs, crates/kakoi-core/src/mounts.rs, crates/kakoi-core/src/mount_facts.rs, crates/kakoi-core/src/guard_placement.rs, Landlock のモジュール, crates/kakoi-net/src/application.rs, crates/kakoi-net/src/supervisor.rs, crates/kakoi-net/src/init.rs, tests/launch.rs, tests/nested.rs, tests/plan.rs, tests/kakoi_net/, "listed" の新しいテストファイル
- Done when: EX-922〜EX-928、EX-947、EX-948、EX-952〜EX-954 がテストで通り、host、none、filtered のどのネットワークのモードでも許していないプログラムが起動できないことがテストで確かめられ、コマンドのモードが "host" のときの今の起動のテストがすべて変わらず通る
- Shown by: test — EX-922、EX-923、EX-924、EX-925、EX-926、EX-927、EX-928、EX-947、EX-948、EX-952、EX-953、EX-954 と、ネットワークのモードごとの起動のテスト
- Left to the implementer: 最初のプロセスが規則を受け取る方法と、filtered で kakoi-net-init の下に最初のプロセスの役を挟む形
- Stop and hand back if: 最初のプロセスを挟むと、既存の要求（REQ-262 の exec の失敗の扱い、REQ-263 の argv0、filtered の終了の扱い）と両立しない。動的リンカの実体を "/lib64/ld-linux-x86-64.so.2" から辿れないホストがテストの環境にある

### S6: 計画の要約と JSON にモードを出す

- Purpose: 計画の要約にマウントのモードと `mounts.system` の 1 行、コマンドのモードと許した項目の数の 1 行を出し、JSON に同じ内容を出し、"format_version" を 1 のままにする
- Specification: docs/ir/core/core-listed-mounts.md#REQ-473, docs/ir/core/core-listed-commands.md#REQ-478
- Prerequisites: S3, S5
- May change: src/plan_text.rs, src/plan_json.rs, crates/kakoi-core/src/plan.rs, tests/plan.rs
- Done when: EX-918、EX-919、EX-929、EX-930 がテストで通り、飛ばした `commands.allow` の項目が数と JSON の項目に含まれないことがテストで確かめられ、今の計画のテストがすべて変わらず通る
- Shown by: test — EX-918、EX-919、EX-929、EX-930
- Left to the implementer: JSON の鍵の名前と、要約の行の言い回し
- Stop and hand back if: なし

### S7: ガードレールの規則に `only` を足す

- Purpose: 規則が `only` を持てるようにし、`deny` 系で止まらなかった起動を `only` で照合し（`for` に当たる起動だけ、規則ごとに別々に）、禁止の 1 行に `only` の場合の当たった語を出す
- Specification: docs/ir/core/core-command-guard-rules.md#REQ-438, docs/ir/core/core-command-guard-rules.md#REQ-443, docs/ir/core/core-command-guard-rules.md#REQ-480, docs/ir/core/core-command-guard-runtime.md#REQ-447
- Prerequisites: S1
- May change: crates/kakoi-core/src/guard.rs, crates/kakoi-core/src/policy.rs, src/guard.rs, tests/guard.rs
- Done when: EX-931、EX-932、EX-933、EX-934 がテストで通り、`only` で禁止になったときの 1 行の当たった語が、読み飛ばしの後の先頭の語（語が無ければプログラム名）であることがテストで確かめられ、`examples.allow` と `examples.deny` の検証が `only` を持つ規則でも働き、今のガードレールのテストがすべて変わらず通る
- Shown by: test — EX-931、EX-932、EX-933、EX-934 と REQ-447 の `only` の 1 行のテスト
- Left to the implementer: なし
- Stop and hand back if: なし

### S8: init で見本を選び、"listed" の見本を同梱する

- Purpose: `kakoi init NAME --example VALUE` を受け付けて選んだ見本を書き出し、`examples/profile/listed.toml` を作って同梱する
- Specification: docs/ir/core/core-init.md#REQ-482, docs/ir/core/core-init.md#REQ-256, docs/ir/core/core-init.md#REQ-259, docs/ir/core/core-bundled-profile.md#REQ-481
- Prerequisites: S3, S5, S7
- May change: src/cli.rs, src/init.rs, src/startup.rs, crates/kakoi-core/src/layers.rs, examples/profile/listed.toml, tests/cli.rs, tests/bundled_profile.rs
- Done when: EX-937、EX-938、EX-939 がテストで通り、REQ-256 と REQ-259 の今のテストが新しい本文で通り、同梱の listed.toml が読み込めて配置の検査を通り、その `ld.so` の規則が引数の無い起動を含むすべての起動を止めることがテストで確かめられる。`ld.so` の規則は `only` に一度も当たらない正規表現の語を 1 つ書く形にする（REQ-438 は空の一覧を形の誤りにするので）
- Shown by: test — EX-937、EX-938、EX-939、REQ-256 と REQ-259 の既存のテスト、同梱の listed.toml の読み込みと `ld.so` の規則のテスト
- Left to the implementer: 見本のコメントの言い回し（REQ-481 が求める内容を含むこと）
- Stop and hand back if: 見本のまま claude か codex が起動しない理由が、見本に書く場所の追加では直らない

### S9: 公開文書、ガイド、変更履歴をそろえる

- Purpose: 公開文書に二つのモードと `only` と `--example` と止め漏れを書き、既知の隙間を 19 件にし、ガイドの古くなった節を直して新しい節を足し、変更履歴と `PROJECT.md` を更新する
- Specification: docs/ir/core/core-listed-commands.md#REQ-479, docs/ir/core/core-public-docs.md#REQ-359
- Prerequisites: S1, S2, S3, S4, S5, S6, S7, S8
- May change: README.md, docs/cli.md, docs/policy.md, docs/security.md, docs/guide/, CHANGELOG.md, PROJECT.md
- Done when: REQ-479 と REQ-359 の how_to_verify の確かめが通り、`docs/security.md` の既知の隙間が TBL-160 の 19 行と一致し、`kotowari check` の guide_stale が 0 件になり（古くなった節を読み直してから fingerprint を写す）、ガイドの参照に "listed" のマウントとコマンドと `only` の節が印付きであり、`CHANGELOG.md` の未リリースの節に三つの機能が書かれ、`PROJECT.md` にテストが Landlock（ABI 6 の scope を含む）を要することが書かれている
- Shown by: check — `kotowari check --format json | jq '[.findings[] | select(.kind == "guide_stale")] | length'` が 0 を返し、REQ-479 と REQ-359 の how_to_verify を読んで確かめる
- Left to the implementer: 文書の節の分け方と言い回し
- Stop and hand back if: ガイドの古くなった節を直すと、IR に無い規則を書くことになる

### S10: 計画の範囲を確かめる

- Purpose: この計画が扱う要求と例に印の付いたテストがあり、ブランチが変えたファイルと計画の ID に誤りが無いことを確かめる
- Specification: docs/ir/core/core-listed-mounts.md#REQ-467, docs/ir/core/core-listed-commands.md#REQ-474, docs/ir/core/core-command-guard-rules.md#REQ-480, docs/ir/core/core-init.md#REQ-482
- Prerequisites: S1, S2, S3, S4, S5, S6, S7, S8, S9
- May change: tests/
- Done when: この計画の Verification map のレビュー以外の要求と例のすべてで `kotowari query ID` の `tests` が空でなく、`kotowari check --format json` がブランチの変えたファイルとこの計画の ID に誤りを出さず、Test command の三つが通る
- Shown by: check — `cargo test --workspace --all-targets --locked`、`cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`kotowari query` を Verification map の各 ID に、`kotowari check --format json`
- Left to the implementer: なし
- Stop and hand back if: 印の無い項目を埋めるテストが、仕様に無い細部を固定しないと書けない
