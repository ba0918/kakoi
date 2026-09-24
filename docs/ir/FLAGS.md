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

### FLAG-011: 省略時のモードの既定値がIRに無い
- kind: gap
- related: REQ-151, REQ-087
- source: docs/decision/brainstorm/2026-09-25-kakoi-spec-retirement.md#A2

旧仕様の policy の文書（第5.1節の形式の例）は「network.mode ... 追加設定が無い場合、省略時 "host"」「env.mode ... 省略時 "inherit"」と定めていた。IRは固定キーとスカラーの上書きを定めるが、どちらの既定値も書いていない。同梱プロファイルは両方を明示しているため、既定値は同梱プロファイルを使わない構成でだけ効く。要求への昇格は別の作業で行う。

### FLAG-012: カレントディレクトリを取得できないときの診断の種類がIRに無い
- kind: gap
- related: REQ-293
- source: docs/decision/brainstorm/2026-09-25-kakoi-spec-retirement.md#A2

旧仕様の process の文書（第13節）は、検査の段階3を「カレントディレクトリの取得（path）」とし、種類 path の診断の条件に「cwd を取得できない」を挙げていた。REQ-293 は段階の順序だけを定め、診断の種類を書いていない。tests/cli.rs の a_deleted_current_directory_is_a_path_diagnostic がこの振る舞いを確かめている。

### FLAG-013: bwrap自身のexecに失敗したときの診断がIRに無い
- kind: gap
- related: REQ-290, REQ-291, REQ-307
- source: docs/decision/brainstorm/2026-09-25-kakoi-spec-retirement.md#A2

旧仕様の process の文書（第13節）は「bwrap 自身の exec に失敗したとき（見つけた後に消された、実行できない）は、まだ kakoi が動いているので種類 bwrap の診断になる」「段階10 ... bwrap の exec（失敗は bwrap、125）」と定めていた。IRが種類 bwrap と定めるのは、bwrap が PATH に無い場合と記述子を用意できない場合だけである。包んだコマンドの exec 失敗は bwrap の失敗としてそのまま返り、入れ子では command not executable になるという非対称の許容も旧仕様にだけある。

### FLAG-014: initが書く範囲と/tmp/kakoiを作らないことがIRに無い
- kind: gap
- related: REQ-256, REQ-305
- source: docs/decision/brainstorm/2026-09-25-kakoi-spec-retirement.md#A2

旧仕様の cli の文書（第4.1節）は「init はファイルを書く唯一の形で、書く先は、設定ディレクトリの規則で組み立てたパスの各成分（無ければ作る）と、その下の profile/、secrets/、書き出すファイルだけである」「/tmp/kakoi は作らない」と定めていた。同梱プロファイルのコメントも /tmp/kakoi を kakoi が作らないと書く。IRの init の要求はこの2点を書いていない。

### FLAG-015: 段階7の内訳の一部の検査がIRに無い
- kind: gap
- related: REQ-294
- source: docs/decision/brainstorm/2026-09-25-kakoi-spec-retirement.md#A2

旧仕様の process の文書（第13節、段階7の内訳）は、3番目を「第6.3節の生成。走査リンクの先の差し替えられる ro、マウント一覧の読込失敗もここで検査」、9番目を「rw-copy の読込。対象種類、読めない複製元、量の上限」と定めていた。REQ-294 は「生成」「rw-copy読込」とだけ書き、その段で出る検査の内訳を持たない。

### FLAG-016: 明示する上流DNSの候補のキーがIRに無い
- kind: gap
- related: REQ-146
- source: docs/decision/brainstorm/2026-09-25-kakoi-spec-retirement.md#A2

旧仕様の network/dns-upstream-config の文書 は次を定めていた。「transport、ip、port は必須。transport は通常DNSの plain または tls、ip はIP本体、port は1〜65535の整数。tls では証明書照合用の tls-name も必須とし、国際化名をASCIIへ正規化する。ワイルドカードは指定できない。plain に tls-name を書くと設定エラーになる。候補の記載順と重複を保持する。」例は [[network.dns-upstream]] に transport = "tls"、ip = "192.0.2.53"、port = 853、tls-name = "resolver.example.com" を書く形だった。REQ-146 は接続IPと証明書の照合名を分けることだけを定める。

### FLAG-017: ホストDNSの細部の規則がIRに無い
- kind: gap
- related: REQ-396, REQ-107, REQ-109
- source: docs/decision/brainstorm/2026-09-25-kakoi-spec-retirement.md#A2

旧仕様の network/dns-host-settings の文書 は次の3点を定めていた。「上流への問い合わせはkakoi自身のネットワークから出るため、ホストのループバック上のDNSも使える。」「kakoiはホストの名前解決設定の内容の変化を変更として検出する。」「問い合わせ先IPと通信方式が同じ場合も、設定変更後は別の解決条件としてやり直せる。」IRは変更の検知の仕組みを未決としており、この3点を書いていない。

### FLAG-018: 許可の登録の再試行とTTLを揃えない応答の細部がIRに無い
- kind: gap
- related: REQ-397, REQ-398, REQ-020
- source: docs/decision/brainstorm/2026-09-25-kakoi-spec-retirement.md#A2

旧仕様の network/dns-lifetime の文書 は「見込んだ時間で登録が間に合わなければ、倍にして登録し直す（上限は残り時間の半分）」「メッセージ署名（TSIG）付きの応答は書き換えられないため、元のTTLのまま返す」と定めていた。REQ-397 は見込む時間を延ばして登録し直すことだけを、REQ-398 は署名付きの応答を書き換えないことだけを書く。REQ-020 の「署名付き」がDNSSECとTSIGのどちらを指すかも、IRからは判断できない。

### FLAG-019: 固定公開の細部の規則がIRに無い
- kind: gap
- related: REQ-393, REQ-394
- source: docs/decision/brainstorm/2026-09-25-kakoi-spec-retirement.md#A2

旧仕様の network/initial-release の文書 は固定公開について次を定めていた。「ホスト側は127.0.0.1または::1だけで待ち受ける。内側の転送先も指定系統のループバックとし、そのアドレスで接続できる全アドレス待受も利用できる。」「一部の公開だけを成功として起動しない。」「設定一覧や外部コマンドの成功終了だけを実際の確保の証拠にはしない。」「環境終了時には既存の終了順序に従って公開を止めて回収する。」「アプリの標準出力は書き換えない。」REQ-393 と REQ-394 は、指定系統のループバックへの転送、起動前の確保の確認、終了までの維持を定めるが、これらの細部を書いていない。

### FLAG-020: 固定公開の既知の制約がIRに無い
- kind: gap
- related: REQ-393
- source: docs/decision/brainstorm/2026-09-25-kakoi-spec-retirement.md#A2, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A169, docs/decision/brainstorm/2026-09-15-kakoi-net.md#A172

旧仕様の network/initial-release の文書 は既知の制約を2つ載せていた。1つ目は「pastaはUDPの流れごとに、ホスト側のソケットへアプリ側の送信元ポートと同じ番号を使う。その番号がホストの別のソケットで使用中の場合、pastaはその流れを作れず、同じ送信元からのデータグラムを捨て続ける。アプリからは応答の無いタイムアウトに見え、kakoiは通知しない。」2つ目は「ホスト自身が持つアドレスは起動時に一度だけ読む。起動後にホストが得たアドレス（VPNの接続など）はホストのアドレスとして扱わず、dns の許可だけで開きうる。」どちらも判断記録にはあるが、IRの要求にも利用者向けの記述にも無い。

### FLAG-021: セットアップスキルの対象とinitの出力内容がIRに無い
- kind: gap
- related: REQ-375, REQ-376
- source: docs/decision/brainstorm/2026-09-25-kakoi-spec-retirement.md#A2

旧仕様の setup-skill の文書 は「書き出すのは組み込みの既定そのもので、full計画の合成後ポリシーと同じ内容である」「導入済みの対象コマンドを調べ、次を提案する。エージェントCLI以外も対象に含む」と定めていた。REQ-375 と REQ-376 はこの2点を書いていない。

### FLAG-022: シムの引数の写しの取りこぼしが境界を広げうる
- kind: contradiction
- related: REQ-372
- source: docs/decision/brainstorm/2026-09-25-kakoi-spec-retirement.md#A2

旧仕様の shim の文書 は「写しの取りこぼしは隔離が緩む向きに働かない（写し先が足りず対象が書けない側に倒れる）」と定めていた。一方、同梱の雛形 examples/shim/codex のヘッダーは、写すオプションと同じ名前の語がどこにあってもそのオプションとして読むため境界が広がりうると書く（codex -m --cd /etc exec で --workspace /etc が kakoi に渡る例）。取りこぼしと誤読は別の現象だが、「緩む向きに働かない」を雛形全体の性質として読むと雛形の記述と矛盾する。実装の挙動は確認していない。

### FLAG-023: 公開文書の掲載内容の細目がIRに無い
- kind: gap
- related: REQ-358, REQ-366, REQ-373
- source: docs/decision/brainstorm/2026-09-25-kakoi-spec-retirement.md#A2

旧仕様の shim の文書 は「"docs/shim.md" には、雛形を写してツール節を埋める手順、同梱の値が codex の例であること、上の表、最初に入りそうな例、保守側と利用者の確かめる条件を載せる」「README のシムの節には、雛形を写す最短の手順と KAKOI_SHIM_OFF=1 の存在、"docs/shim.md" への導線を載せる」と定めていた。旧仕様の documentation の文書 は「本仕様の他の節で『README に書く』『README に載せる』とあるものは、README か上の文書のいずれかに書くことを指す」と定めていた。IRはこれらを書いていない。

### FLAG-024: codex以外のツール節を同梱しないことがIRに無い
- kind: gap
- related: REQ-353, REQ-368
- source: docs/decision/brainstorm/2026-09-25-kakoi-spec-retirement.md#A2

旧仕様の overview の文書（第18節）は、0.3で作らないものに「codex 以外のツール節の値（opencode、claude など）。実測していないものは同梱しない。利用者が雛形を写してツール節を埋める」を挙げていた。REQ-353 の対象外の一覧にこの項目は無い。

### FLAG-025: ネットワークの入力エラーの診断の種類と終了コードが定まっていない
- kind: ambiguity
- related: REQ-084, REQ-005, REQ-146, REQ-394
- source: docs/decision/brainstorm/2026-09-25-kakoi-spec-retirement.md#A2

filtered のIRは、ポリシーの誤りを「入力エラー」「設定エラー」「起動前エラー」、起動時の失敗を「起動エラー」とだけ書き、診断の種類（policy など）と終了コードを定めていない。本体の REQ-290 から policy と125を当てはめる読み方はあるが、IRは明示していない。旧仕様にも明示は無かった。

### FLAG-026: IRが旧仕様の節の一覧を参照している
- kind: gap
- related: REQ-355, REQ-356, REQ-359
- source: docs/decision/brainstorm/2026-09-25-kakoi-spec-retirement.md#A2

REQ-355 と REQ-356 は「仕様第15.1節の全項目」「第15.2節の全観測」、REQ-359 は「仕様第16節に継承する既知の隙間15件」を参照する。旧仕様の削除後、これらの一覧の本文はガイドの検証の契約と公開文書の章にだけある。ガイドは正本ではないので、一覧をIRへ移すか、要求の文言を一覧に依存しない形にする必要がある。
