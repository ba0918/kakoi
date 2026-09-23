# 本体仕様の抽出で見つかった問題

本体の既存仕様・既存テストを照合して見つかった事項。記載は不具合の断定や新しい仕様判断ではない。今回、製品とテストの動作は変更しない。

### FLAG-001: 配置診断の理由が部分的に未検証
- kind: gap
- related: REQ-158
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A3

tests/placement.rs の a_policy_file_inside_a_writable_area_is_rejected_with_the_three_reasons は、assert_path_diagnostic に問題のパスと書き込み項目のパスだけを渡す。仕様が求める第三の要素「隔離内から差し替えられるため」という理由は確認していない。要求IDが付いていても、理由の説明が欠落する退行をこのテストだけでは検出できない。実装の不具合は未確認。後続の修正では自由な説明文の字面を固定せず、契約上の理由を伝えていることを検証する。

### FLAG-002: 保護対象の診断順位が裁量の記述と競合する
- kind: contradiction
- related: REQ-158
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A3

旧第5.6節は保護対象の3規則を「この順」に検査し最初を出すと定める。一方、旧第20節の実装裁量には「保護対象の3つの規則のどれを出すか」が含まれる。複数に該当するときの診断が固定か裁量かで競合する。今回どちらかへ寄せず既存記述を保持した。利用者に見える診断順位を決める際に整理が必要。

### FLAG-003: シム検出手順のcommand -vの説明が不正確
- kind: contradiction
- related: REQ-382
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A3

旧第16節は「command -vはシェルの関数とaliasを見ない」と説明する。しかしこの環境のBashで関数kakoi_probe_functionを定義して command -v kakoi_probe_function を実行すると、パスではなく関数名を返した。少なくとも関数を見ないという説明は成立しない。aliasの扱いを含む全対象シェルの確認は未実施。関数・aliasの横取りを別途調べる既存の手順は維持し、今回新たな探索手順を決めない。

### FLAG-004: roの既存テストの名前とコメントが古い
- kind: ambiguity
- related: REQ-161
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A3

tests/placement.rs の an_ro_written_through_a_writable_link_is_accepted_wherever_it_lands_outside_hide_and_ro の名前とコメントは、roにroを重ねる形も露出として拒否すると読める。既存仕様と a_referenced_ro_replacing_a_lower_layer_ro_is_accepted は、その形の許可で一致している。assert同士の動作矛盾は確認していない。後続作業で名前・コメントの更新を検討する。

### FLAG-005: initの既存ファイル拒否で書込み禁止の範囲が曖昧
- kind: ambiguity
- related: REQ-258
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A3

旧第4.1節は書き出し先がある場合「何も書かず」に拒否すると定める。一方src/init.rsのwrite_built_in_defaultは、secretsの作成とchmod0700を済ませてから書き出し先の既存判定をする。tests/cli.rsのinit_refuses_to_overwrite_an_existing_profileはプロファイルの内容不変だけを確認する。「何も」が出力ファイルだけか、ディレクトリ作成・モード変更も含むかで適否が変わる。今回の抽出ではこの語の範囲を決めず、実測では既存プロファイルを保ったままpath・125で終了し、secretsのモードが0755から0700に変わった。製品の副作用は確認済みだが、契約違反かは文言の範囲判断を残す。

### FLAG-006: 通常の端末操作の許容を直接検証していない
- kind: gap
- related: REQ-283
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A3

既存テストはTIOCSTIとx32の拒否を確認しているが、端末サイズ取得のioctlの許容と制御端末の維持を直接確認するテストを今回の対応付けでは発見できなかった。拒否対象を広げ過ぎて通常の端末操作を壊す退行の検出が不足する可能性がある。実際に壊れているという証拠ではない。

### FLAG-007: 既存テストが仕様以上の細部を確認する
- kind: gap
- related: REQ-155, REQ-170
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A3

tests/mounts.rsのa_name_the_isolation_planted_cannot_stall_a_pattern_with_many_starsは、星の多いパターンが255文字の名前に一致しないという結果に加え1秒以内の完了を確認する。既存仕様は照合の意味を定めるが1秒という上限を定めていない。a_mount_point_with_a_byte_that_is_not_utf8_does_not_empty_the_mount_listは非UTF-8のマウント名を保持することを確認するが、既存仕様ではその入力境界を明示していない。両テストの動作は維持し、特定の上限や表現を今回の新要求として追認しないため印を付けていない。テスト不在ではなく、テストと明文化された要求の対応不足である。

### FLAG-008: ホーム全体の複製が必ず上限超過するとは限らない
- kind: ambiguity
- related: REQ-161, REQ-168
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A3

旧第5.6節はホーム全体のrw-copyが複製上限に当たりpathで止まると断定する。しかし第6.1節の上限は4096エントリ・64MiBであり、それ以内の小さなホームまで必ず拒否されるとは導けない。ホームのrw-copyを広さの拒否対象から外す規則は保持する。上限超過は典型例の説明なのか、別の無条件拒否を意図したのかを今回の文章整理で決めない。

### FLAG-009: JSONの出所の列挙がどの欄に適用されるか曖昧
- kind: ambiguity
- related: REQ-301
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A3

旧第13節は出所のkindをprofile等8種類で列挙する。一方、tests/cli.rsのJSONテストは項目のorigin.kindをprofile/secretと確認し、policy_sources[0].kindをfileと確認する。列挙が項目のoriginだけを指すのか、policy_sourcesも含むのかが原文で明確でない。今回kindの追加やテスト変更は行わず、内側のJSON契約を明確にする課題を残す。

### FLAG-010: 全形式と名付けた表示テストがfullを確認していない
- kind: gap
- related: REQ-297
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A3

tests/cli.rsのrw-copy表示テストはevery_formという名前を持つが、実際の呼び出しはsummaryとjsonでfullを含まない。全量形式にも複製の説明と項目が出るという条件を、このテストだけでは確認できない。印とテスト名を完全な網羅の証拠として扱わない。
