# Plan: 実装済みの要求と例を、仕様を満たすかで確かめるテストでそろえる

## Goal

実装済みなのにテストの印が無い要求と例（後回しの草案と、下の「人に返す項目」を除く 103 件）のそれぞれが、仕様の述べる振る舞いを製品の外から確かめるテストで覆われ、`kotowari check` のそれらの指摘が消えた状態にする。

## Specification

IR は `docs/ir/`。この計画が扱う項目は次のとおり。どれも承認・コミット済みで、多くは旧仕様から移した要求（決定記録 `docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md` の A1）である。

- `docs/ir/core/core-cli-syntax.md`: EX-490〜EX-494
- `docs/ir/core/core-init.md`: EX-495〜EX-499
- `docs/ir/core/core-command-resolution.md`: EX-500〜EX-504
- `docs/ir/core/core-environment.md`: EX-505〜EX-515
- `docs/ir/core/core-mounts.md`: EX-336〜EX-349
- `docs/ir/core/core-policy.md`: EX-350〜EX-363
- `docs/ir/core/core-policy-placement.md`: EX-364〜EX-371
- `docs/ir/core/core-output.md`: EX-525〜EX-529
- `docs/ir/core/core-plan-output.md`: EX-530〜EX-537
- `docs/ir/core/core-terminal.md`: `#REQ-283`、EX-516〜EX-519
- `docs/ir/core/core-process.md`: `#REQ-286`、EX-520〜EX-524
- `docs/ir/core/core-runtime.md`: `#REQ-313`、`#REQ-314`、EX-538〜EX-549
- `docs/ir/core/core-network-existing.md`: EX-716、EX-717
- `docs/ir/network/dns-capacity/network-dns-coalescing.md`: EX-290

例の本文は `kotowari query EX-nnn`、例が指す要求（`@about`）の本文は `kotowari query REQ-nnn` で読む。今の一覧は `kotowari check --format json | jq -r '.findings[] | select(.kind == "requirement_without_test" or .kind == "scenario_without_test") | .detail'` から、後回しの草案（`docs/ir/network/publish-config/`、`publish-lifetime/`、`publish-target/`、`link-local/`、`policy/network-inactive-validation.md`）と、Out of scope の EX-271 を除いたものである。`network-inactive-validation.md` は文書自身が「後続の動的公開の入力検査」と書いているので、後回しの草案に入れる。

## Approach and why

テストは実装の振る舞いではなく、仕様を満たすかを確かめる。実装を読んで「今こう動いているからこう書く」テストは、実装が仕様から外れていても通り、仕様が変わっても何も知らせないので、テストの形をした無価値なものになる。そこで各項目について次の順で進める。

- 先に仕様を読む。例の Given、When、Then と、`@about` の要求の本文から、例ごとに「作る条件」と「期待する結果」を書き出す。実装はこの段階では読まない。書き出しは結果の報告に例ごとに残し、レビュー役が照合できるようにする。
- テストは例の Then の各句（And を含む）をすべて主張し、かつ仕様に無い細部は主張しない。上限と下限の両方である。終了コードだけを見て診断の種類を見ない、警告と続行の片方だけを見る、といった弱いテストには印を付けない。期待する結果は、仕様が契約として述べるものだけを確かめる。診断の種類（`usage`、`policy`、`path`、`secret`、`env`、`bwrap` など）、終了コード、標準出力に出るかどうか、マウントされるかどうか、仕様が値を定めた文字列やパスなど。仕様が定めていない説明文の言い回し、内部の関数名や型、実装がたまたま取る順序や中間状態は確かめない。診断は `tests/common` の `assert_diagnostic`（種類、終了コード、標準出力が空であること）で確かめ、説明文は比べない（`docs/ir/core/core-process.md` が「種類は契約、説明は自由文」と定める）。
- 観測は製品の外の境界から行う。S1〜S6 は組み立てた `kakoi` の実行ファイル（`tests/common` の `binary`）と、実際のファイルシステムに作った条件だけで確かめ、計画は `--print-plan=json` の出力で見る。`tests/common/fixture.rs` の `MountFacts` のような、手で組み立てた事実で条件を作るテストには印を付けない。ライブラリの公開 API から確かめてよいのは S7 の EX-290（kakoi-net の DNS の公開 API。`tests/kakoi_net/dns_gate.rs` の方式）だけとする。
- 書き出しの後で既存のテストを探す。既存のテストに印を付けてよいのは、その主張が書き出しと一致する（Then の各句をすべて主張し、仕様に無い細部を主張しない）場合だけ。報告には例ごとに、書き出し、採用したテストの名前、新しく書いたか既存に印を付けたかを並べる。テストが仕様に無い細部まで固定しているときは、そのテストに印を付けず、仕様の範囲だけを確かめる新しいテストを書く。既存のテストは消さず、仕様に無い細部を固定していることを報告に挙げる（消すかどうかは人が決める）。
- 例の Given が「本体の既存仕様を適用する」のように具体的な条件を持たない場合は、`@about` の要求の本文から条件を取る。要求の本文からも観測する条件を一つに決められない場合は、止めて返す。

仕様どおりのテストが製品で失敗したら、それは製品が仕様を満たしていない発見である。製品のコードは直さない（仕様と実装のどちらを直すかは人が決める）。失敗したテストはコミットせず、テストのコードと失敗の出力を `.agents/tmp/spec-tests-core-failures/` に例の ID ごとに保存して報告に挙げ、ほかの項目は続ける。

証拠条件: テスト、検査、フィクスチャが証拠になるのは、それが作る条件に、対応する環境の中で実際にその条件を生む者がいて（境界に届く信頼できない入力もその一つ）、対象がテスト自身でなく製品か検査であり、確かめる規則を仕様が述べていて、固定する言い回し・ファイル配置・内部名がすべて仕様で契約として宣言されているときだけ。条件を作るためにテストの側で偽の事実や代用品を組み立てて済ませてはならない。この条件を満たせない例は、テストを書かずに報告に挙げる。

作りにくい例の観測の形は次のとおり決める。

- EX-504（bwrap の実行の失敗をそのまま返す）: 同じ条件で bwrap を直接実行した結果（終了コードと標準エラー）と kakoi の結果が一致することで確かめる（`tests/launch.rs` にある方式）。bwrap の文言や番号を固定しない。
- EX-516（i386 の番号のシステムコール）: x86_64 のプロセスが `int 0x80` で出すシステムコールを条件とする。この環境のカーネルで i386 の呼び出しが使えないなら、テストを書かずに報告に挙げる。seccomp のフィルタの命令の並びは契約ではないので確かめない。
- EX-538（保存しない）: 実行の前後で設定ディレクトリと HOME の下に kakoi が作ったファイルが無いことで確かめる。
- EX-539（HEAD の存在と種類を調べる）: 外から見える結果（EX-353 の結果）と、HEAD の内容を読めなくしても同じ結果になること（REQ-306 の「HEAD 内容は読まない」）で確かめる。
- REQ-283、REQ-286、REQ-313、REQ-314（要求として印で覆う 4 件）: 本文の句ごとに、テストで確かめる句と、README に書かれているかを読んで報告する句を分けて報告に書く。テストで確かめる句がすべてテストで覆われたときだけ要求に印を付ける。端末が要る句（REQ-283 の制御端末の維持）など、対応する環境で条件を作れない句は報告に挙げる。

テストの印は kotowari の mark の場面（kotowari スキルの `references/mark.md`）に従い、テスト関数の直前に `// @kotowari[REQ-nnn, EX-nnn]` の形で置く。印を付けるのは、そのテストが実際に確かめている項目だけにする。

## Scope of change

- `tests/` の下のテストファイル（既存ファイルへの追加を基本とする）
  - `tests/cli.rs`、`tests/launch.rs`、`tests/mounts.rs`、`tests/policy.rs`、`tests/placement.rs`、`tests/plan.rs`、`tests/layers.rs`、`tests/variables.rs`、`tests/isolated_env.rs`、`tests/seccomp.rs`、`tests/bundled_profile.rs`
  - `tests/kakoi_net/` の下の既存のファイル（新しいファイルを足すなら `tests/kakoi_net.rs` の `mod` も）
  - テスト用の共通の手助け（`tests/common/`）は、足すテストに要る範囲だけ
- 製品のコード、IR、ガイド、README は変えない

各手順の May change は、この範囲の `tests/` 全体とする（既存のテストがどのファイルにあるかは手順の区切りと一致しないため）。

## Step order and prerequisites

S1 から S7 は互いに独立で、どの順でもよい。S8 は最後に行う。

## Verification map

| Step | 読む仕様（例の `@about` の要求） | 印で覆う対象 |
|---|---|---|
| S1 | REQ-250〜REQ-264 | EX-490〜EX-504 |
| S2 | REQ-268〜REQ-277 | EX-505〜EX-514 |
| S3 | REQ-167〜REQ-173 | EX-336〜EX-349 |
| S4 | REQ-151〜REQ-161 | EX-350〜EX-371、EX-515 |
| S5 | REQ-289〜REQ-301 | EX-525〜EX-537 |
| S6 | REQ-280〜REQ-288、REQ-305〜REQ-316 | REQ-283、REQ-286、REQ-313、REQ-314、EX-516〜EX-524、EX-538〜EX-549 |
| S7 | REQ-388、REQ-127 | EX-716、EX-717、EX-290 |
| S8 | 全体の確認 | なし |

## Left to the implementer

- 新しいテストの名前と、どのテストファイルに置くか（上の範囲の中で）
- 同じ条件と観測を共有する複数の例を、1 つのテストにまとめるかどうか（まとめても、テストが確かめることは各例の Then を超えない）

## Stop conditions

仕様どおりのテストの失敗と、証拠条件を満たせない例は、止めずに報告に集める（Approach のとおり）。次の場合は止めて返す。

- 例と `@about` の要求の本文から、観測する条件か期待する結果を一つに決められない
- 製品のコード、IR、ガイド、README を変える必要が出る

## Test command

```sh
cargo test --workspace --all-targets --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

この開発環境の PATH の pasta は、確かめた版（`~/.local/bin/pasta`）である。

## Out of scope

- 後回しの草案（動的公開の `docs/ir/network/publish-config/`、`publish-lifetime/`、`publish-target/`、`docs/ir/network/policy/network-inactive-validation.md` と、`docs/ir/network/link-local/`）の要求と例。kotowari に後回しの仕組みが入ってから扱う
- EX-271（通信層のパケット再送を DNS 問い合わせとして数えない）。数えないのはカーネルの TCP 再送でアプリから見えず、応答の無い上流では kakoi-net 自身の再試行（数える側）になるため、外から確かめる形を人が決める
- `tests/mounts.rs` の印の無い 2 つのテスト（`docs/ir/FLAGS.md` の FLAG-007）。仕様に無い細部を確かめているため、仕様を足すかテストを直すかを人が決める
- 仕様に無い細部を固定している既存のテストの削除や書き直し（報告だけにする）

## Steps

### S1: CLI の文法、init、コマンドの解決の例を確かめる

- Purpose: CLI の文法、`init`、コマンドの解決の例を、仕様を満たすかを確かめるテストで覆う
- Specification: `docs/ir/core/core-cli-syntax.md#REQ-250`、`docs/ir/core/core-cli-syntax.md#REQ-251`、`docs/ir/core/core-cli-syntax.md#REQ-252`、`docs/ir/core/core-cli-syntax.md#REQ-253`、`docs/ir/core/core-cli-syntax.md#REQ-254`、`docs/ir/core/core-init.md#REQ-255`、`docs/ir/core/core-init.md#REQ-256`、`docs/ir/core/core-init.md#REQ-257`、`docs/ir/core/core-init.md#REQ-258`、`docs/ir/core/core-init.md#REQ-259`、`docs/ir/core/core-command-resolution.md#REQ-260`、`docs/ir/core/core-command-resolution.md#REQ-261`、`docs/ir/core/core-command-resolution.md#REQ-262`、`docs/ir/core/core-command-resolution.md#REQ-263`、`docs/ir/core/core-command-resolution.md#REQ-264`
- Prerequisites: none
- May change: `tests/` の下（Scope of change のとおり）
- Done when: EX-490〜EX-504 のそれぞれが、Then の各句をすべて主張し仕様に無い細部を主張しないテストの印に含まれてそのテストが通るか、証拠条件を満たせない・仕様どおりのテストが失敗したとして報告に挙がっている
- Shown by: test — 例ごと（または条件と観測を共有する例のまとまりごと）のテスト。報告には例ごとに、仕様から書き出した作る条件と期待する結果、採用したテストの名前、新しく書いたか既存に印を付けたかを並べる
- Left to the implementer: none
- Stop and hand back if: 例の Then と要求の本文が食い違う

### S2: 環境変数の例を確かめる

- Purpose: 隔離の中へ渡す環境変数の例を、仕様を満たすかを確かめるテストで覆う
- Specification: `docs/ir/core/core-environment.md#REQ-268`、`docs/ir/core/core-environment.md#REQ-269`、`docs/ir/core/core-environment.md#REQ-270`、`docs/ir/core/core-environment.md#REQ-271`、`docs/ir/core/core-environment.md#REQ-272`、`docs/ir/core/core-environment.md#REQ-273`、`docs/ir/core/core-environment.md#REQ-274`、`docs/ir/core/core-environment.md#REQ-275`、`docs/ir/core/core-environment.md#REQ-276`、`docs/ir/core/core-environment.md#REQ-277`
- Prerequisites: none
- May change: `tests/` の下（Scope of change のとおり）
- Done when: EX-505〜EX-514 のそれぞれが、Then の各句をすべて主張し仕様に無い細部を主張しないテストの印に含まれてそのテストが通るか、証拠条件を満たせない・仕様どおりのテストが失敗したとして報告に挙がっている
- Shown by: test — S1 と同じ形
- Left to the implementer: none
- Stop and hand back if: 例の Then と要求の本文が食い違う

### S3: マウントの例を確かめる

- Purpose: マウントの指令と計画の例を、仕様を満たすかを確かめるテストで覆う
- Specification: `docs/ir/core/core-mounts.md#REQ-167`、`docs/ir/core/core-mounts.md#REQ-168`、`docs/ir/core/core-mounts.md#REQ-169`、`docs/ir/core/core-mounts.md#REQ-170`、`docs/ir/core/core-mounts.md#REQ-171`、`docs/ir/core/core-mounts.md#REQ-172`、`docs/ir/core/core-mounts.md#REQ-173`
- Prerequisites: none
- May change: `tests/` の下（Scope of change のとおり）
- Done when: EX-336〜EX-349 のそれぞれが、Then の各句をすべて主張し仕様に無い細部を主張しないテストの印に含まれてそのテストが通るか、証拠条件を満たせない・仕様どおりのテストが失敗したとして報告に挙がっている
- Shown by: test — S1 と同じ形。FLAG-007 の 2 つのテストには印を付けない
- Left to the implementer: none
- Stop and hand back if: 例の Then と要求の本文が食い違う

### S4: ポリシーと配置の例を確かめる

- Purpose: ポリシーの読み込みと合成、ポリシーの置き場所の検査の例を、仕様を満たすかを確かめるテストで覆う（EX-515 は EX-355 と同じ振る舞いなので、ここで 1 つのテストに両方の印を付けてよい）
- Specification: `docs/ir/core/core-policy.md#REQ-151`、`docs/ir/core/core-policy.md#REQ-152`、`docs/ir/core/core-policy.md#REQ-153`、`docs/ir/core/core-policy.md#REQ-154`、`docs/ir/core/core-policy.md#REQ-155`、`docs/ir/core/core-policy.md#REQ-156`、`docs/ir/core/core-policy.md#REQ-157`、`docs/ir/core/core-policy-placement.md#REQ-158`、`docs/ir/core/core-policy-placement.md#REQ-159`、`docs/ir/core/core-policy-placement.md#REQ-160`、`docs/ir/core/core-policy-placement.md#REQ-161`
- Prerequisites: none
- May change: `tests/` の下（Scope of change のとおり）
- Done when: EX-350〜EX-371 と EX-515 のそれぞれが、Then の各句をすべて主張し仕様に無い細部を主張しないテストの印に含まれてそのテストが通るか、証拠条件を満たせない・仕様どおりのテストが失敗したとして報告に挙がっている
- Shown by: test — S1 と同じ形
- Left to the implementer: none
- Stop and hand back if: 例の Then と要求の本文が食い違う

### S5: 出力と計画表示の例を確かめる

- Purpose: 診断と出力、計画表示の例を、仕様を満たすかを確かめるテストで覆う
- Specification: `docs/ir/core/core-output.md#REQ-289`、`docs/ir/core/core-output.md#REQ-290`、`docs/ir/core/core-output.md#REQ-291`、`docs/ir/core/core-output.md#REQ-292`、`docs/ir/core/core-output.md#REQ-293`、`docs/ir/core/core-plan-output.md#REQ-294`、`docs/ir/core/core-plan-output.md#REQ-295`、`docs/ir/core/core-plan-output.md#REQ-296`、`docs/ir/core/core-plan-output.md#REQ-297`、`docs/ir/core/core-plan-output.md#REQ-298`、`docs/ir/core/core-plan-output.md#REQ-299`、`docs/ir/core/core-plan-output.md#REQ-300`、`docs/ir/core/core-plan-output.md#REQ-301`
- Prerequisites: none
- May change: `tests/` の下（Scope of change のとおり）
- Done when: EX-525〜EX-537 のそれぞれが、Then の各句をすべて主張し仕様に無い細部を主張しないテストの印に含まれてそのテストが通るか、証拠条件を満たせない・仕様どおりのテストが失敗したとして報告に挙がっている
- Shown by: test — S1 と同じ形
- Left to the implementer: none
- Stop and hand back if: 例の Then と要求の本文が食い違う

### S6: 端末、プロセス、実行時の要求と例を確かめる

- Purpose: 端末とシステムコールの制限、プロセス、実行時の信頼の前提とパス解決の要求と例を、仕様を満たすかを確かめるテストで覆う
- Specification: `docs/ir/core/core-terminal.md#REQ-280`、`docs/ir/core/core-terminal.md#REQ-281`、`docs/ir/core/core-terminal.md#REQ-282`、`docs/ir/core/core-terminal.md#REQ-283`、`docs/ir/core/core-process.md#REQ-284`、`docs/ir/core/core-process.md#REQ-285`、`docs/ir/core/core-process.md#REQ-286`、`docs/ir/core/core-process.md#REQ-287`、`docs/ir/core/core-process.md#REQ-288`、`docs/ir/core/core-runtime.md#REQ-305`、`docs/ir/core/core-runtime.md#REQ-306`、`docs/ir/core/core-runtime.md#REQ-307`、`docs/ir/core/core-runtime.md#REQ-308`、`docs/ir/core/core-runtime.md#REQ-309`、`docs/ir/core/core-runtime.md#REQ-310`、`docs/ir/core/core-runtime.md#REQ-311`、`docs/ir/core/core-runtime.md#REQ-312`、`docs/ir/core/core-runtime.md#REQ-313`、`docs/ir/core/core-runtime.md#REQ-314`、`docs/ir/core/core-runtime.md#REQ-315`、`docs/ir/core/core-runtime.md#REQ-316`
- Prerequisites: none
- May change: `tests/` の下（Scope of change のとおり）
- Done when: EX-516〜EX-524、EX-538〜EX-549 のそれぞれが、Then の各句をすべて主張し仕様に無い細部を主張しないテストの印に含まれてそのテストが通るか報告に挙がっており、REQ-283、REQ-286、REQ-313、REQ-314 は Approach のとおり句ごとに分けて報告され、テストで確かめる句がすべて覆われたものに印が付いている
- Shown by: test — S1 と同じ形
- Left to the implementer: none
- Stop and hand back if: 例の Then と要求の本文が食い違う

### S7: ネットワークの既存の振る舞いと DNS の解決の共有の例を確かめる

- Purpose: none の経路と、進行中の DNS の解決を共有するときの許可の確認の例を、仕様を満たすかを確かめるテストで覆う
- Specification: `docs/ir/core/core-network-existing.md#REQ-388`、`docs/ir/network/dns-capacity/network-dns-coalescing.md#REQ-127`
- Prerequisites: none
- May change: `tests/` の下（Scope of change のとおり）
- Done when: EX-716、EX-717、EX-290 のそれぞれが、Then の各句をすべて主張し仕様に無い細部を主張しないテストの印に含まれてそのテストが通るか、報告に挙がっている
- Shown by: test — EX-716、EX-717 は実行ファイルで none の隔離から接続を試みる。EX-290 は kakoi-net の DNS の公開 API（`tests/kakoi_net/dns_gate.rs` の方式）で、進行中の解決がある状態で、許可されない問い合わせ元からの問い合わせが共有されないことを確かめる
- Left to the implementer: none
- Stop and hand back if: EX-290 の「進行中の解決」を公開 API で作れない

### S8: 全体を確かめる

- Purpose: この計画の項目がそろってテストで覆われ、既存のテストが壊れていないことを示す
- Specification: `docs/ir/core/core-terminal.md#REQ-283`、`docs/ir/core/core-process.md#REQ-286`、`docs/ir/core/core-runtime.md#REQ-313`、`docs/ir/core/core-runtime.md#REQ-314`、`docs/ir/core/core-network-existing.md#REQ-388`（ほかの項目は S1〜S7 の Specification のとおり）
- Prerequisites: S1、S2、S3、S4、S5、S6、S7
- May change: none
- Done when: Test command がすべて通り、この計画の Specification に挙げた要求と例の `kotowari query` の tests が空でなく、`kotowari check` の requirement_without_test と scenario_without_test の残りが、後回しの草案、EX-271、報告に挙げた項目だけになっている
- Shown by: check — Test command の 3 つ、`kotowari check --format json | jq -r '.findings[] | select(.kind == "requirement_without_test" or .kind == "scenario_without_test") | select(.path | test("publish-(config|lifetime|target)/|link-local/|network-inactive-validation") | not) | .detail'` の出力が、EX-271 と、報告に挙げた項目だけであること
- Left to the implementer: none
- Stop and hand back if: 報告に挙げていない残りが出る
