# Plan: 古い pasta を名指しで診断し、新しい pasta の入れ方を案内する

## Goal

必要なオプションを持たない pasta や PATH に無い pasta で filtered を起動した利用者が、足りないオプションの名前と "docs/pasta.md" の URL を診断で受け取り、その文書の手順で新しい pasta を入れられる状態にする。

## Specification

IR は `docs/ir/`。この計画が扱う要求は `docs/ir/network/network-pasta-install.md` の次の 7 つで、根拠は決定記録 `docs/decision/brainstorm/2026-09-25-pasta-install.md`（A1〜A22）にある。

- 製品の挙動: `docs/ir/network/network-pasta-install.md#REQ-429`、`#REQ-430`、`#REQ-431`、`#REQ-432`
- レビューで確かめる要求: `docs/ir/network/network-pasta-install.md#REQ-433`、`#REQ-434`、`#REQ-435`
- 例: EX-824〜EX-845（REQ-433〜435 の例 EX-833〜EX-838 はレビューで確かめる要求だけを指すので、テストの印は要らない）

各要求と例の本文は `kotowari query REQ-nnn`、`kotowari query EX-nnn` で読む。

## Approach and why

古さの判定は kakoi-net の filtered の起動（`crates/kakoi-net/src/filtered.rs` の `start` で `Transport::start_closed` の失敗を受ける箇所）に置く。ここは最初の起動だけが通り、通信障害からの立ち直り（`crates/kakoi-net/src/transport.rs` の再起動）は通らないので、REQ-431 の「再起動の失敗では確かめない」が置き場所で満たされる。`--print-plan` はこの関数に来る前に終わるので、何もしなくても REQ-431 の計画表示の条件を満たす。

判定には「準備完了の合図より前に pasta が終了した」ことを、タイムアウト、PID の不正、取り消しと区別して知る必要がある。今の `crates/kakoi-net/src/pasta.rs` はどれも `io::Error` の文言でしか区別しておらず、`crates/kakoi-net/src/child_output.rs` の `with_diagnostic` が pasta の標準エラーを文言に平らにしている。起動の失敗の種類と pasta の標準エラーを別々に持ち帰れるようにする（REQ-430 は pasta の出力を含めず、REQ-432 は含める）。`Transport::start_closed` と `Pasta::start` の公開の返り型（`io::Result`）は変えず、既存の失敗の文言（"pasta exited during startup" など）と `ErrorKind`（再起動の期限切れの `TimedOut` など）も保つ。`tests/kakoi_net/session.rs` がそれらを確かめている。起動用のパイプが閉じた（EOF）ときの待ち方（REQ-430 の括弧書き、REQ-431）と `--help` を待つ上限（REQ-432）は仕様が決めている。待つ間も `is_running` と同じく標準エラーを読み続け、標準エラーのパイプが満杯で pasta が止まらないようにする（`tests/kakoi_net/pasta.rs` の `startup_failure_preserves_bounded_diagnostics_without_blocking_on_a_full_pipe` が守っている性質）。EOF の後の待ちは `Pasta::start_controlled` の中に置くと再起動の経路にも効くので、そこでも取り消しを待ちの中で見る。

確かめる名前は、pasta の引数を組み立てるときと同じ一覧から取る（REQ-430）。名前の前後の「空白」はスペースとタブの両方を指す。実物の pasta の `--help` は名前の後ろにタブを置く行が多い（`--quiet`、`--foreground`、`--no-map-gw`、`--host-lo-to-ns-lo`、`--config-net`。確かめた版と Ubuntu 24.04 の版の両方で観測）。今は長い名前が `Pasta::start_controlled` の中に直に書かれているので、2 段で渡す長い名前（`--foreground`、`--pid`、`--config-net`、`--quiet`、`--host-lo-to-ns-lo`、`--userns`、`--netns`、`--no-map-gw`、`--map-host-loopback`）を 1 か所の一覧にまとめ、引数の組み立てと判定の両方がそれを使う形にする。短い名前（`-T`、`-U`、`-I`、`-i`、`-t`、`-u`）は一覧に入れない。

`--help` は、起動に使ったのと同じ実行ファイルのパスをホストの側で実行し、標準出力と標準エラーを合わせて読む（Ubuntu 24.04 の pasta は使い方を標準出力に出して 0 で終わる）。

テストは、`tests/kakoi_net/cli.rs` の `Filtered` の仕組み（PATH の先頭に置いた偽の pasta と、本物の bwrap、nft、名前空間）を使い、偽の pasta の振る舞いを例ごとに変える。偽の pasta は実物に合わせ、起動の失敗の理由と使い方の文は標準エラーに出し（標準出力は準備完了の合図のパイプである）、`--help` の使い方は標準出力に出してタブで終わる名前の行を含める。偽の pasta は Ubuntu 24.04 の標準の pasta という、実在する古い pasta の振る舞いを写すもので、REQ-430 と REQ-432 が契約として宣言した診断の種類、終了コード、説明文に含む名前と URL だけを確かめる。

テストの印は kotowari の mark の場面（kotowari スキルの `references/mark.md`）に従い、テスト関数の直前に `// @kotowari[REQ-nnn, EX-nnn]` の形で置く。

## Scope of change

- 製品のコード
  - `crates/kakoi-net/src/pasta.rs`（長い名前の一覧、起動の失敗の種類、EOF 後の終了の確認）
  - `crates/kakoi-net/src/filtered.rs`（PATH に無いときの URL、古さの判定と診断）
  - `crates/kakoi-net/src/transport.rs`（起動の失敗の種類を `start_closed` から持ち帰るのに要る範囲だけ）
- テスト
  - `tests/kakoi_net/cli.rs`、`tests/kakoi_net/plan.rs`、`tests/kakoi_net/pasta.rs`、`tests/kakoi_net/session.rs`
  - `tests/kakoi_net/` の下に置く 2 つの pasta の `--help` の出力の写し（新規）
- 文書
  - `docs/pasta.md`（新規、英語）
  - `README.md`（filtered の依存の記述から "docs/pasta.md" へのリンク）
  - `skills/kakoi-setup/SKILL.md`（pasta の確認）
  - `docs/guide/reference/network/01-modes.md`、`docs/guide/reference/12-setup-skill.md`、`docs/guide/maintainer/public-docs.md`（ガイドの節とガイド印）

## Step order and prerequisites

S1 を最初に行う。S2 は S1 の URL の定数を使う。S3 は S2 の起動の失敗の種類と判定を使う。S4 は S2 の長い名前の一覧を文書に写すので S2 の後。S5 は S4 の文書を指すので S4 の後。S6 はすべての要求の最終の姿を説明するので S5 の後。S7 は最後。

## Verification map

| Step | 要求 | 例 |
|---|---|---|
| S1 | REQ-429 | EX-824、EX-825、EX-839 |
| S2 | REQ-430、REQ-432 | EX-826、EX-827、EX-828、EX-840、EX-841、EX-831、EX-832、EX-844、EX-845 |
| S3 | REQ-431 | EX-829、EX-830、EX-842、EX-843 |
| S4 | REQ-434（レビュー） | EX-835、EX-836 |
| S5 | REQ-435（レビュー） | EX-837、EX-838 |
| S6 | REQ-429〜REQ-435 のガイドの説明 | EX-824〜EX-845（ガイド印） |
| S7 | REQ-433（レビュー）、全体の確認 | EX-833、EX-834 |

## Left to the implementer

- 起動の失敗の種類を表す型の名前と形、判定と診断を組み立てる関数の名前と置き場所（kakoi-net の中）
- 長い名前の一覧の置き方（定数の配列、列挙など）
- 診断の説明文のうち、契約（種類 bwrap、125、足りない名前すべて、URL、pasta の出力を含まない）の外の言い回し
- "docs/pasta.md" の構成と英語の言い回し（REQ-434 が挙げる内容を満たす範囲）

## Stop conditions

- 上流の passt の git（"https://passt.top/passt"）から取得できない、またはタグ "2026_07_28.f8df3f1" が無い
- 起動の失敗の種類を持ち帰るために、`Transport` や `Session` の公開された使い方を、この計画の範囲の外まで変える必要が出る
- EOF の後に終了を待つ変更で、既存のテスト（`tests/kakoi_net/pasta.rs` の起動の失敗のテスト、`tests/kakoi_net/session.rs` の再起動のテスト）の期待が変わる
- 既存のテストが、この計画の変更と関係のない理由で失敗する
- 起動の失敗の種類を持ち帰るために、既存の失敗の文言か `ErrorKind` を変える必要が出る

## Test command

```sh
KAKOI_TEST_PASTA=<確かめた版の pasta のパス> cargo test --workspace --all-targets --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

本物の pasta で通信を運ぶテストは、PATH の pasta ではなく `KAKOI_TEST_PASTA` の pasta を使う。この開発環境では確かめた版を `.agents/tmp/kakoi-net-probes/passt-debian-backports/root/usr/bin/pasta` に置いてある（PATH の `/usr/bin/pasta` は古い）。

## Out of scope

- 古さ以外の理由で pasta が落ちたときに、pasta の出力がそのまま長く出る問題（決定記録の A11）
- pasta の同梱、古い pasta への対応、pasta の場所を指定する設定（R1、R2、A8）
- CI の変更（A15）
- CHANGELOG（版を切るときに書く）

## Steps

### S1: PATH に pasta が無いときの診断に導入手順の URL を載せる

- Purpose: pasta が PATH に無い利用者を "docs/pasta.md" へ案内する
- Specification: `docs/ir/network/network-pasta-install.md#REQ-429`
- Prerequisites: none
- May change: `crates/kakoi-net/src/filtered.rs`、`tests/kakoi_net/cli.rs`
- Done when: PATH に pasta が無いと種類 bwrap、125 で、説明文に "https://github.com/ba0918/kakoi/blob/main/docs/pasta.md" を含み、pasta も nft も無いときも pasta について述べて URL を含む
- Shown by: test — 既存の `a_missing_pasta_ends_the_start_before_the_application_runs` に URL の確認を足して EX-824、EX-825 の印を付け、pasta と nft がともに無い PATH で起動する新しいテストに EX-839 の印を付ける
- Left to the implementer: URL を持つ定数の名前と置き場所（S2 の診断と共有する）
- Stop and hand back if: nft が無いときの既存の診断の文言や順序を変える必要が出る

### S2: 必要なオプションを持たない pasta を名指しで診断する

- Purpose: 起動中に終了した pasta を `--help` で調べ、足りない長い名前があれば専用の診断にし、古さ以外なら今の診断のままにする
- Specification: `docs/ir/network/network-pasta-install.md#REQ-430`、`docs/ir/network/network-pasta-install.md#REQ-432`
- Prerequisites: S1
- May change: `crates/kakoi-net/src/pasta.rs`、`crates/kakoi-net/src/filtered.rs`、`crates/kakoi-net/src/transport.rs`、`crates/kakoi-net/src/child_output.rs`、`tests/kakoi_net/cli.rs`、`tests/kakoi_net/pasta.rs`、`tests/kakoi_net/` の下に置く `--help` の出力の写し（新規）
- Done when: 長い名前の一覧が 1 か所にあり引数の組み立てと判定が同じ一覧を使い、偽の pasta で EX-826、EX-827、EX-828、EX-840、EX-841、EX-831、EX-832、EX-844、EX-845 のとおりの種類、終了コード、説明文になり、確かめた版の pasta の実際の `--help` の出力ではどの名前も足りず、Ubuntu 24.04 の標準の pasta の実際の `--help` の出力では "--host-lo-to-ns-lo" と "--map-host-loopback" だけが足りないと判定され、既存のテストがすべて通る
- Shown by: test — 例ごとに 1 つのテスト（EX-826 と EX-827 は同じ偽の pasta で 1 つにまとめてよい）。偽の pasta は、起動で理由と使い方の文を標準エラーに出して終了し `--help` で使い方を標準出力に出す形（EX-826）、起動用のパイプを閉じてから 0.5 秒待って終了する形（EX-841。すぐ終了すると変更前でも終了が先に見えて RED にならない）、`--help` が空や 0 以外で終わる形（EX-832、EX-844）、`--help` が終わらない形（EX-845。起動の期限の 10 秒を待つ）を例の Given のとおりに作る。照合の処理そのものは、実物の 2 つの pasta の `--help` の出力を写した入力で確かめるテストも置く。写しは、この開発環境の `.agents/tmp/kakoi-net-probes/passt-debian-backports/root/usr/bin/pasta --help` と `/usr/bin/pasta --help`（Ubuntu 24.04 の passt 0.0~git20240220.1e6f92b-1）の標準出力をそのまま保存したもので、どちらの版から取ったかを写しの置き場所の名前か隣のコメントで分かるようにする（CI には Ubuntu の標準の passt が無く、テストは飛ばさずに失敗させる約束なので、実行時に取らず写しを持つ）
- Left to the implementer: `--help` の出力の写しのファイル名と置き場所、起動の失敗の種類を表す型、EOF の後に終了を待つ方法と `--help` の出力を読む方法（どちらも残りの起動の期限を上限とすることは仕様が決めている）
- Stop and hand back if: 内側の段の pasta（名前空間の中で起動される）だけが落ちる場合に、ホストの側で同じパスの `--help` を実行できない

### S3: 古さを確かめない場合をテストで固定する

- Purpose: 起動に成功したとき、タイムアウトのとき、立ち直りの再起動が失敗したとき、`--print-plan` のときに `--help` を実行しないことを確かめる
- Specification: `docs/ir/network/network-pasta-install.md#REQ-431`
- Prerequisites: S2
- May change: `tests/kakoi_net/cli.rs`、`tests/kakoi_net/plan.rs`、`tests/kakoi_net/session.rs`
- Done when: 呼ばれた引数を記録する偽の pasta で、起動に成功した実行（EX-829、EX-830）、起動がタイムアウトした実行（EX-842）、準備完了の合図に違う PID を返した実行、起動用のパイプを閉じて動き続けた実行、`--print-plan` の実行（EX-843）、立ち直りの再起動が失敗した実行のどれにも `--help` の呼び出しの記録が無い
- Shown by: test — EX-829 と EX-830 を 1 つのテスト、EX-842 を 1 つのテスト（CLI で起動の期限の 10 秒まで待つ）、PID の不正を 1 つのテスト、パイプを閉じて動き続ける pasta を 1 つのテスト（起動の期限の 10 秒を待つ。どちらも REQ-431 の印）、EX-843 は `tests/kakoi_net/plan.rs` の既存の `filtered_plan_shows_normalized_intent_without_starting_network_tools`（pasta の呼び出しの記録 "pasta.called" が無いことを既に確かめている）に印を付ける。立ち直りは `tests/kakoi_net/session.rs` の `automatic_recovery_waits_after_failure_and_retries_without_user_input` で、偽の pasta が引数を読む前に全引数を別のファイルへ記録するようにし、`--help` の記録が無いことを確かめて REQ-431 の印を付ける（判定が再起動の経路へ動いたときに捕まえるため）
- Left to the implementer: none
- Stop and hand back if: 既存の再起動のテストに記録の確認を足すと、そのテストが確かめている要求の観測が変わる

### S4: pasta の導入手順の文書を置き、README から案内する

- Purpose: 利用者が自分の pasta を確かめ、足りなければタグからビルドして入れられる英語の文書を置く
- Specification: `docs/ir/network/network-pasta-install.md#REQ-434`
- Prerequisites: S2
- May change: `docs/pasta.md`、`README.md`
- Done when: "docs/pasta.md" が REQ-434 の how_to_verify の内容をすべて満たし、確かめ方の名前の一覧が S2 の長い名前の一覧と一致し、README の filtered の依存の記述から "docs/pasta.md" へリンクしている
- Shown by: external — 文書のソースビルドの手順を、この開発環境の一時ディレクトリで書かれたとおりに実行し（置き先は `~/.local/bin` の代わりに一時ディレクトリの prefix にしてよい）、置いた pasta の `--help` に文書が載せた名前がすべて載ることを観測して、実行したコマンドと出力を結果の報告に残す
- Left to the implementer: 文書の見出しと構成、Ubuntu 23.10 以降の制限を案内する一文の置き場所
- Stop and hand back if: 手順どおりに実行してもビルドや配置が成功しない、または置いた pasta の `--help` に名前が揃わない

### S5: セットアップスキルに pasta の確認を足す

- Purpose: filtered を使う利用者に、スキルが pasta の古さを調べて "docs/pasta.md" の手順を示すようにする
- Specification: `docs/ir/network/network-pasta-install.md#REQ-435`
- Prerequisites: S4
- May change: `skills/kakoi-setup/SKILL.md`
- Done when: SKILL.md が REQ-435 の how_to_verify の内容をすべて満たし、`agentskills validate skills/kakoi-setup` が通る
- Shown by: check — `agentskills validate skills/kakoi-setup` の後、SKILL.md を REQ-435 の how_to_verify に沿って読む
- Left to the implementer: SKILL.md の中の置き場所と言い回し
- Stop and hand back if: 書き先を設定ディレクトリと承認したシムの置き場に限る既存の規則（REQ-379）と食い違う書き方が要る

### S6: ガイドに pasta の診断、導入の文書、スキルの確認を書く

- Purpose: 日本語のリファレンスガイドに REQ-429〜REQ-435 の説明とガイド印を置く
- Specification: `docs/ir/network/network-pasta-install.md#REQ-429`、`docs/ir/network/network-pasta-install.md#REQ-430`、`docs/ir/network/network-pasta-install.md#REQ-431`、`docs/ir/network/network-pasta-install.md#REQ-432`、`docs/ir/network/network-pasta-install.md#REQ-433`、`docs/ir/network/network-pasta-install.md#REQ-434`、`docs/ir/network/network-pasta-install.md#REQ-435`
- Prerequisites: S5
- May change: `docs/guide/reference/network/01-modes.md`、`docs/guide/reference/12-setup-skill.md`、`docs/guide/maintainer/public-docs.md`
- Done when: 7 つの要求と例のそれぞれが、どれかの節のガイド印に `kotowari query` の fingerprint とともに載り、`kotowari check` がガイドに guide_stale も invalid_marker も出さない
- Shown by: check — `kotowari check --format json | jq '[.findings[] | select(.path | startswith("docs/guide/"))]'` が空であること
- Left to the implementer: 節の分け方（1 つのガイド印を数個の ID に保つ）
- Stop and hand back if: ガイドに書くために IR に無い規則を足す必要が出る

### S7: 全体を確かめる

- Purpose: この計画の要求と例がそろって確かめられ、ブランチの変更に kotowari の誤りが無いことを示す
- Specification: `docs/ir/network/network-pasta-install.md#REQ-433`
- Prerequisites: S1、S2、S3、S4、S5、S6
- May change: none
- Done when: Test command がすべて通り、REQ-429〜REQ-432 と EX-824〜EX-832、EX-839〜EX-845 の `kotowari query` の tests が空でなく、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に誤りを出さず、REQ-433 の how_to_verify に沿った読み取りで pasta の場所を指定する手段が無いことを確かめた
- Shown by: check — Test command の 3 つ、`for id in REQ-429 REQ-430 REQ-431 REQ-432 EX-824 EX-825 EX-826 EX-827 EX-828 EX-829 EX-830 EX-831 EX-832 EX-839 EX-840 EX-841 EX-842 EX-843 EX-844 EX-845; do kotowari query $id | jq -e '.items[0].tests != []' > /dev/null || echo "no test: $id"; done` が何も出さないこと、`kotowari check --format json` の findings のうち severity が error のものに、network-pasta-install.md とブランチで変えたファイルのものが無いこと（network-pasta-install.md の too_many_lines の notice は決定記録に残す理由を書いて受け入れ済み）
- Left to the implementer: none
- Stop and hand back if: REQ-433 の読み取りで、pasta の場所を PATH 以外から取る経路が見つかる
