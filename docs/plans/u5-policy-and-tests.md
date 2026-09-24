# Plan: filtered の host-interface を policy で拒否し、旧仕様から移した要求をテストで確かめる

## Goal

filtered で `host-interface` を含む宛先を書いた利用者が、起動前（`--print-plan` を含む）に種類 policy の診断を受け取り、旧仕様から IR に移した要求のそれぞれが、印の付いたテストで確かめられている状態にする。

## Specification

IR は `docs/ir/`。この計画が扱う要求と、その根拠の決定記録は次のとおり。

- 実装を変える要求: `docs/ir/network/policy/network-input-diagnostics.md#REQ-428`（決定記録 `docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md` の A1、A2）
- テストで確かめる要求（今の実装が既に従っているもの。決定記録 `docs/decision/brainstorm/2026-09-25-spec-only-rules.md`）
  - `docs/ir/core/core-policy.md#REQ-399`
  - `docs/ir/core/core-output.md#REQ-400`、`docs/ir/core/core-output.md#REQ-401`
  - `docs/ir/core/core-plan-output.md#REQ-403`、`docs/ir/core/core-plan-output.md#REQ-404`
  - `docs/ir/network/dns-upstream-config/network-dns-upstream-config.md#REQ-415`、`#REQ-416`
  - `docs/ir/network/dns-host-settings/network-dns-host-startup.md#REQ-417`
  - `docs/ir/network/dns-host-settings/network-dns-settings-change.md#REQ-418`、`#REQ-419`
  - `docs/ir/network/dns-lifetime/network-dns-lifetime.md#REQ-420`、`#REQ-398`（EX-802、EX-803）
  - `docs/ir/network/dns-trust/network-dns-answer.md#REQ-020`（EX-806、EX-807）
  - `docs/ir/network/network-initial-release.md#REQ-421`、`#REQ-422`、`#REQ-423`
  - `docs/ir/network/host/network-host-nonloopback.md#REQ-425`
  - `docs/ir/network/policy/network-input-diagnostics.md#REQ-426`、`#REQ-427`

各要求の本文と例は `kotowari query REQ-nnn` で読む。例（EX）の本文は `kotowari query EX-nnn` で読む。

## Approach and why

実装を変えるのは REQ-428 だけで、ほかの要求は今の実装が既に従っている。旧仕様にだけ書かれていた振る舞いを IR に移したので、テストは挙動を固定する目的で書く。RED の段階は「印が無い」か「その条件を観測するテストが無い」ことで、GREEN はテストが通って印が付いた状態である。既存のテストが例の観測をそのまま行っているなら、テストを足さずに印だけを付ける。

REQ-428 は、合成後のモードが決まる場所（kakoi-core の段の合成、`crates/kakoi-core/src/layers.rs`）で拒否する。`--print-plan` も同じ合成を通るので（検査の段階 5）、計画の表示でも同じ診断になる。今の拒否は kakoi-net の起動の処理（`crates/kakoi-net/src/filtered.rs` の `filter_rules` と `unsupported`）にあり、種類 bwrap を返しているので、合成で先に止まるようにする。

テストの印は kotowari の mark の場面（kotowari スキルの `references/mark.md`）に従い、テスト関数の直前に `// @kotowari[REQ-nnn, EX-nnn]` の形で置く。印を付けるのは、そのテストが実際に確かめている要求と例だけにする。

## Scope of change

- `crates/kakoi-core/src/layers.rs`（REQ-428 の拒否を足す）
- `crates/kakoi-net/src/filtered.rs`（REQ-428 で到達しなくなる拒否の扱い）
- `tests/` の下のテストファイル（既存ファイルへの追加を基本とする）
  - `tests/layers.rs`、`tests/cli.rs`、`tests/plan.rs`、`tests/mounts.rs`、`tests/launch.rs`
  - `tests/kakoi_net/` の `cli.rs`、`config.rs`、`upstream.rs`、`host_dns.rs`、`filter.rs`、`dns_response.rs`、`publish.rs`、`host.rs`
  - テスト用の共通の手助け（`tests/common/`、`tests/kakoi_net/fake_host.rs`）は、足すテストに要る範囲だけ

製品のコードで変えてよいのは上の 2 ファイルだけである。

## Step order and prerequisites

S1 を最初に行う。REQ-428 は製品の挙動を変える唯一の変更で、S5 の REQ-427 のテスト（起動の処理で見つかる誤りが bwrap になること）が、host-interface を例に使わない形になっていることを前提にするためである。S2 から S7 は互いに独立で、どの順でもよい。S8 は最後に行う。

## Verification map

| Step | 要求 | 例 |
|---|---|---|
| S1 | REQ-428 | EX-820、EX-822、EX-823 |
| S2 | REQ-399、REQ-400、REQ-401 | EX-750〜EX-756 |
| S3 | REQ-403、REQ-404 | EX-759〜EX-762 |
| S4 | REQ-415、REQ-416、REQ-426 | EX-790〜EX-795、EX-818、EX-819 |
| S5 | REQ-417、REQ-418、REQ-419、REQ-427 | EX-796〜EX-801、EX-821 |
| S6 | REQ-420、REQ-398、REQ-020 | EX-802〜EX-807 |
| S7 | REQ-421、REQ-422、REQ-423、REQ-425 | EX-808〜EX-813、EX-816、EX-817 |
| S8 | この表のすべて | この表のすべて |

verification が review の要求（REQ-405〜REQ-409、REQ-424、REQ-359 など）と、その例（EX-736、EX-737、EX-763〜EX-776、EX-814、EX-815）はテストを持たないので、この計画の対象外である。

## Left to the implementer

- テスト関数の名前（振る舞いを述べる文にする）と、既存ファイルのどこに置くか
- 1 つのテストで複数の例を観測するか、例ごとにテストを分けるか
- REQ-428 の拒否を入れたあと、kakoi-net に残る host-interface の拒否を消すか、到達しない防御として残すか（どちらでも観測できる振る舞いは REQ-428 のとおり）
- 診断の説明の文言（種類 policy と終了コード 125 が契約で、説明文は契約ではない）

## Stop conditions

- 既存のテストが、この計画の変更と関係ない理由で失敗する
- 例（EX）の文と今の実装の振る舞いが食い違う（テストを実装に合わせて書き換えず、どの例がどう食い違うかを持ち帰る）
- テストの前提（pasta、nftables、`/dev/net/tun`、unshare、bwrap）が無い環境で、ネットワークのテストが失敗ではなく実行できない（PROJECT.md はこれらを必須としている）

## Test command

```sh
cargo test --workspace --all-targets --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

## Out of scope

- IR、ガイド、決定記録の変更（この計画は要求を変えない）
- CHANGELOG の記載（版を切るときに行う）
- 動的公開の要求（REQ-033〜REQ-081 の一部）、REQ-283、REQ-286、REQ-313、REQ-314 など、この計画の前からテストの無い要求

## Steps

### S1: filtered の host-interface を段の合成で policy として拒否する

- Purpose: 合成後のモードが filtered のとき、host-interface を含む宛先を種類 policy・終了コード 125 で拒否し、`--print-plan` でも同じ診断にし、host と none では通す
- Specification: `docs/ir/network/policy/network-input-diagnostics.md#REQ-428`
- Prerequisites: none
- May change: `crates/kakoi-core/src/layers.rs`, `crates/kakoi-net/src/filtered.rs`, `tests/kakoi_net/cli.rs`, `tests/kakoi_net/config.rs`
- Done when: filtered で `destination = { ip = "fe80::1", host-interface = "eth0" }` を書いたポリシーで起動すると種類 policy・終了コード 125 でアプリを起動せず終わり、`--print-plan` を付けても計画を出さず同じ診断で終わり、host で同じ許可を書くと診断なしで起動する
- Shown by: test — EX-820 と EX-822 を観測する新しいテスト（RED は今の実装が bwrap を返すことで確認する）と、EX-823 を観測する既存の `unused_settings_are_neither_checked_nor_resolved`（tests/kakoi_net/cli.rs）への印
- Left to the implementer: 拒否の判定を合成の関数の中のどこに置くか
- Stop and hand back if: 合成の段階ではモードがまだ決まらず、合成後のモードを見て拒否できる場所が layers.rs に無い

### S2: 省略時のモード、cwd の失敗、bwrap 自身の exec の失敗をテストで確かめる

- Purpose: REQ-399、REQ-400、REQ-401 の各例を観測するテストに印を付け、足りない観測にはテストを足す
- Specification: `docs/ir/core/core-policy.md#REQ-399`, `docs/ir/core/core-output.md#REQ-400`, `docs/ir/core/core-output.md#REQ-401`
- Prerequisites: none
- May change: `tests/layers.rs`, `tests/cli.rs`, `tests/launch.rs`, `tests/plan.rs`, `tests/common/`
- Done when: EX-750〜EX-756 のそれぞれを観測するテストが通り、印が付いている
- Shown by: test — 既存の `scalars_take_the_upper_layer`（tests/layers.rs）と `a_deleted_current_directory_is_a_path_diagnostic`（tests/cli.rs）と `a_command_inside_a_hidden_directory_fails_at_exec_with_bwrap_status`（tests/launch.rs）が観測する例には印を付け、EX-752（mode が無く追加のキーがあると policy）と EX-755（所在確認を通った bwrap の exec が失敗すると bwrap・125。存在しないインタプリタを指すスクリプトを bwrap として PATH に置く）には新しいテストを足す
- Left to the implementer: EX-755 の偽の bwrap の作り方（存在しないインタプリタを指すスクリプトでも、実行形式でないファイルでも、所在確認を通って exec で失敗すればよい）
- Stop and hand back if: 偽の bwrap の exec の失敗が bwrap の診断ではなく別の種類になる

### S3: 段階 7 の生成と rw-copy の読み込みの検査をテストで確かめる

- Purpose: REQ-403、REQ-404 の各例を観測するテストに印を付け、足りない観測にはテストを足す
- Specification: `docs/ir/core/core-plan-output.md#REQ-403`, `docs/ir/core/core-plan-output.md#REQ-404`
- Prerequisites: none
- May change: `tests/plan.rs`, `tests/mounts.rs`, `tests/launch.rs`, `tests/common/`
- Done when: EX-759〜EX-762 のそれぞれを観測するテストが通り、印が付いている
- Shown by: test — 既存の `a_scan_link_into_a_swappable_ro_item_is_a_path_diagnostic`（tests/plan.rs）、`an_unreadable_mount_list_with_hide_mounts_is_a_path_diagnostic`（tests/mounts.rs）、`an_rw_copy_source_over_the_entry_limit_is_a_path_diagnostic` と `an_rw_copy_of_something_that_is_neither_a_directory_nor_a_regular_file_is_a_path_diagnostic`（tests/launch.rs）が観測する例に印を付ける。例が観測していない条件（読めない複製元、通常ファイルの内容の合計 64MiB の上限）は、例が求める範囲でだけ足す
- Left to the implementer: none
- Stop and hand back if: テストを root で走らせる環境で、読めない複製元をファイルのモードで作れない

### S4: 上流 DNS の候補の書き方と、読み込みで見つかる誤りの種類を確かめる

- Purpose: REQ-415、REQ-416 の例と、REQ-426 の例（読み込みと合成で見つかる誤りが種類 policy・125）を観測するテストに印を付け、種類を確かめていないテストには種類の確認を足す
- Specification: `docs/ir/network/dns-upstream-config/network-dns-upstream-config.md#REQ-415`, `docs/ir/network/dns-upstream-config/network-dns-upstream-config.md#REQ-416`, `docs/ir/network/policy/network-input-diagnostics.md#REQ-426`
- Prerequisites: none
- May change: `tests/kakoi_net/upstream.rs`, `tests/kakoi_net/config.rs`, `tests/kakoi_net/cli.rs`
- Done when: EX-790〜EX-795、EX-818、EX-819 のそれぞれを観測するテストが通り、印が付いている。EX-818 と EX-819 は、起動した kakoi の診断の種類が policy で終了コードが 125 であることまで確かめている
- Shown by: test — 既存の `upstream_configuration_separates_connection_address_from_tls_identity` と `upstream_layers_append_preserving_order_and_reject_mixed_transport`（tests/kakoi_net/upstream.rs）に印を付け、EX-818 と EX-819 は起動した kakoi の出力を見るテストで確かめる
- Left to the implementer: none
- Stop and hand back if: 読み込みか合成で見つかる上流の誤りが policy 以外の種類になる

### S5: ホスト DNS の上流の選び方と変更の判定、起動の処理で見つかる誤りを確かめる

- Purpose: REQ-417〜REQ-419 と REQ-427 の例を観測するテストに印を付け、足りない観測にはテストを足す
- Specification: `docs/ir/network/dns-host-settings/network-dns-host-startup.md#REQ-417`, `docs/ir/network/dns-host-settings/network-dns-settings-change.md#REQ-418`, `docs/ir/network/dns-host-settings/network-dns-settings-change.md#REQ-419`, `docs/ir/network/policy/network-input-diagnostics.md#REQ-427`
- Prerequisites: S1
- May change: `tests/kakoi_net/host_dns.rs`, `tests/kakoi_net/cli.rs`, `tests/kakoi_net/fake_host.rs`
- Done when: EX-796〜EX-801、EX-821 のそれぞれを観測するテストが通り、印が付いている。EX-799（内容が変わらない書き直しは変更としない）と EX-800（問い合わせ先が同じでも変更後はやり直す）は、それぞれ専用のテストで観測している
- Shown by: test — ホストのループバック上の上流で解決する既存の `following` モジュールのテスト（tests/kakoi_net/host_dns.rs）と `a_host_dns_change_redoes_pending_queries_and_ignores_old_answers` に印を付け、EX-799、EX-800、EX-821（名前解決設定に nameserver が無いと bwrap・125）には新しいテストを足す
- Left to the implementer: none
- Stop and hand back if: 同じ内容の書き直しでも問い合わせがやり直される、または問い合わせ先が同じ変更でやり直されない

### S6: 許可の登録のやり直しと、署名付きの応答の扱いを確かめる

- Purpose: REQ-420 の例と、REQ-398 と REQ-020 の署名の書き分け（EX-802、EX-803、EX-806、EX-807）を観測するテストに印を付け、足りない観測にはテストを足す
- Specification: `docs/ir/network/dns-lifetime/network-dns-lifetime.md#REQ-420`, `docs/ir/network/dns-lifetime/network-dns-lifetime.md#REQ-398`, `docs/ir/network/dns-trust/network-dns-answer.md#REQ-020`
- Prerequisites: none
- May change: `tests/kakoi_net/filter.rs`, `tests/kakoi_net/dns_response.rs`
- Done when: EX-802〜EX-807 のそれぞれを観測するテストが通り、印が付いている。TSIG 付きの応答が元の TTL のまま返ること（EX-802）と、TSIG 付きの混在応答を加工しないこと（EX-807）を観測するテストがある
- Shown by: test — 既存の `a_late_nft_widens_the_staging_reserve_instead_of_faulting_the_owner` と `a_late_staging_never_leaves_the_permission_past_the_answer_expiry`（tests/kakoi_net/filter.rs）と `every_answer_reaches_the_application_with_the_shortest_time_to_live`（tests/kakoi_net/dns_response.rs）が観測する例に印を付け、EX-802、EX-807 と、見込みを倍にすることと残り時間の半分の上限（EX-804、EX-805）を既存のテストが観測していなければ新しいテストを足す
- Left to the implementer: none
- Stop and hand back if: 見込みを倍にすることや上限を、公開された入口から観測する手段が無く、内部の関数名を固定するテストでしか確かめられない

### S7: 固定公開の細部と、ホスト自身のアドレスを一度だけ読むことを確かめる

- Purpose: REQ-421〜REQ-423 と REQ-425 の例を観測するテストに印を付け、足りない観測にはテストを足す
- Specification: `docs/ir/network/network-initial-release.md#REQ-421`, `docs/ir/network/network-initial-release.md#REQ-422`, `docs/ir/network/network-initial-release.md#REQ-423`, `docs/ir/network/host/network-host-nonloopback.md#REQ-425`
- Prerequisites: none
- May change: `tests/kakoi_net/publish.rs`, `tests/kakoi_net/host.rs`, `tests/kakoi_net/fake_host.rs`
- Done when: EX-808〜EX-813、EX-816、EX-817 のそれぞれを観測するテストが通り、印が付いている
- Shown by: test — 既存の `fixed_publish_lifetime` と `fixed_publish_partial_failure_prevents_launch`（tests/kakoi_net/publish.rs）が観測する例に印を付け、アプリの標準出力をそのまま渡すこと（EX-812、EX-813）と、起動後にホストが得たアドレスを dns の許可で開くこと（EX-816、EX-817）には新しいテストを足す
- Left to the implementer: EX-817 で、起動後にホストへアドレスを足す方法（テスト用のホストの名前空間の中で足す）
- Stop and hand back if: テスト用のホストの名前空間で起動後にアドレスを足す手段が無い

### S8: 計画が扱う要求と例に印が付き、変更したファイルに誤りが無いことを確かめる

- Purpose: この計画の対象がすべてテストで確かめられ、変更が検査を通ることを示す
- Specification: `docs/ir/network/policy/network-input-diagnostics.md#REQ-428`, `docs/ir/core/core-policy.md#REQ-399`, `docs/ir/network/network-initial-release.md#REQ-421`
- Prerequisites: S1, S2, S3, S4, S5, S6, S7
- May change: none
- Done when: Verification map のすべての要求と例について `kotowari query ID | jq '.items[0].tests'` が空でなく、`kotowari check` がこのブランチで変えたファイルとこの計画の ID に誤りを出さず、Test command の 3 つがすべて成功する
- Shown by: check — Test command の 3 つを順に実行し、Verification map の各 ID に `kotowari query ID | jq '.items[0].tests | length'` を実行して 0 でないことを確かめ、`kotowari check | jq '[.findings[] | select(.severity=="error")]'` の中にこのブランチで変えたファイルか計画の ID を指すものが無いことを確かめる
- Left to the implementer: none
- Stop and hand back if: 計画の外の要求の印やテストを変えないと検査が通らない
