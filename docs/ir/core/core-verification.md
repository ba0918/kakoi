# 検証の境界

既存仕様から継承した本体・補助物の契約。この IR が正本である。

## Requirements

### REQ-354: 計画算出の純粋な境界
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: crates/kakoi-coreの計画算出の関数を読み、入力が合成前の各段と出所、CLI引数、ホスト環境、収集済みの事実だけであり、事実に候補パスの存在・種類・実体、マウント一覧と取得失敗、走査結果、Git相互リンク、秘密の内容、パス解決で参照したもの、rw-copyの木、cwdが含まれることを確かめる。その関数がファイルシステムやプロセスに触れず、事実の収集、記述子の割り当て、bwrapの起動が関数の外にあることを確かめる。

計画算出は合成前の各段と出所、CLI引数、ホスト環境、収集済み事実だけを入力にする純粋関数にする。事実には候補パスの存在・種類・実体、マウント一覧と取得失敗、走査結果、Git相互リンク、秘密の内容、パス解決で参照したもの、rw-copyの木、cwdを含む。事実収集、記述子割当、bwrap起動は関数の外に置く。

### REQ-355: 純粋関数の検証範囲
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A1
- verification: review
- how_to_verify: 表 TBL-156 と表 TBL-157 の各行について、純粋関数のテストに成り立つ条件と対になる成り立たない境界の両方を確かめるテストがあるかを、テストと突き合わせて人か LLM が確かめる

純粋関数の検証では、表 TBL-156 と表 TBL-157 の全行を含める。各行は、成り立つ条件だけでなく、対になる成り立たない境界も確かめる。組み込みの既定への置き換えはファイルを読み込むときに決まるので、純粋関数の外で表 TBL-158 に従って観測する。

### REQ-356: 実バイナリの検証範囲
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A1
- verification: review
- how_to_verify: 表 TBL-158 と表 TBL-159 の各行について、ビルド済みのkakoiと実bwrapを起動して観測するテストがあり、外部ネットワークに接続せずに通るかを、テストと突き合わせて人か LLM が確かめる

Linux x86_64でビルド済みkakoiと実bwrapを起動し、表 TBL-158 と表 TBL-159 の全観測条件を確認する。テストは外部ネットワーク接続に依存しない。

### REQ-357: 移行時の一度限りの確認
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: 現行のcodex用jailから移行するとき、利用者の実機で旧スクリプトのbwrap引数と kakoi --print-plan の出力を並べ、マウント集合が一致することを人が一度確かめる。旧スクリプトが継続して動くテストに含まれていないことをテストの構成で確かめる。

現行codex用jailからの移行時は、利用者の実機で旧スクリプトのbwrap引数とprint-planを並べ、マウント集合が一致することを人が一度確認する。旧スクリプトは製品でないため継続テストにしない。

## Decision tables

### TBL-156: 純粋関数で確かめる項目
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A1

| 区分 | 確かめる条件 |
|---|---|
| 設定、変数、合成 | "${git_common_dir}" の相互リンクが成り立つ場合と成り立たない場合。共通ディレクトリ C に通常ファイルの "HEAD" が無い場合も含める |
| 設定、変数、合成 | 3 種類の合成規則、"path-prepend" の向き、ワイルドカードの意味 |
| 設定、変数、合成 | パスの同一性。存在しないパスを展開後の文字列で判定する場合も含める |
| 設定、変数、合成 | 変数の展開と、相対パスの拒否 |
| 設定、変数、合成 | "init" の文法。NAME の制約と、他の引数との併用を含める |
| ポリシーを保護する配置 | 表 TBL-157 の全行。拒否だけでなく、許可すべき境界も確かめる |
| マウントと複製 | "rw-copy" を、ホストへ書き込める項目に数えない。中に置いたポリシーファイル、設定ディレクトリ、秘密ファイルが保護対象の検査を通ること、根の項目の検査の対象外であること、作業場所への警告で "rw" として数えないことを確かめる |
| マウントと複製 | "rw-copy" の複製元が無ければ飛ばす。生成する引数は、ディレクトリの tmpfs とエントリの順、通常ファイルの "--bind-data"、モードの引き継ぎを確かめる |
| マウントと複製 | 生成の段と走査は、シンボリックリンクとポリシーファイルの除外を含める。差し替えられない "ro" の中を指すリンクは隠さず、差し替えられうる "ro" の中を指すリンクは "path" の診断になることを確かめる |
| マウントと複製 | "hide-mounts" は、作業場所の除外と、マウント一覧を読めないという事実からの "path" の診断を確かめる。設定ディレクトリの "secrets/" の隠しも確かめる |
| マウントと複製 | マウントの 6 場面の重なりと、兄弟の順序 |
| マウントと複製 | 作業場所への警告と、2 種類の拒否（広すぎる位置、隠されたカレントディレクトリ）。再び見えるようにしたカレントディレクトリは許可する |
| 環境、起動、表示 | 環境の構成の 7 段階、"unset" のワイルドカード、"path-prepend" の実体化と飛ばし |
| 環境、起動、表示 | 秘密の注入の 2 段階、LF と CR LF の扱い、秘密ファイルが無いときの警告、値の上限 |
| 環境、起動、表示 | Git の設定の番号付け。空の "GIT_CONFIG_COUNT" も含める |
| 環境、起動、表示 | 出力に埋め込む制御文字のエスケープ |
| 環境、起動、表示 | コマンドの解決と、bwrap 引数列の "--argv0" および末尾の "--" |

### TBL-157: ポリシーを保護する配置で確かめる境界
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A1

| 対象 | 確かめる条件 |
|---|---|
| 保護対象のパス | 3 つの検査すべて。秘密ファイルも対象に含める |
| 根の項目の検査の対象 | "rw"、"rw-file"、"hide"、明示した workspace、走査の "root"、"hide-mounts" の "under" |
| 生成前の起点の検査 | "root" と "under" は生成前の集合で判定し、その診断が後続の検査より先に出る |
| パス解決で参照したもの | 入れ子の項目、".." で書き込める領域を通り抜けるリンク先、workspace の変数からの参照の引き継ぎ。走査の "root" への引き継ぎも含める |
| workspace | カレントディレクトリと同じ実体を指す指定は除外する。リンクを辿る明示の指定の規則も確かめる |
| "hide" のリンク | 書き込める領域の中のリンクを辿る "hide" は、着地先が "rw" の外でも根の項目の中でも拒否する。"rw" の中を実体のパスで指定した "hide" は許可する |
| 走査の起点 | "rw" の下位の "root" は拒否し、"rw" のマウント点そのものの "root" は許可する。"rw" の中のリンクを経由してマウント点を指す "root" と "under" は拒否する |
| 根の項目の検査の対象外 | "ro" と "rw-copy" のリンクは、どの項目でもない場所へ向けても通る |
| 固定部分と広さ | "/"、"/dev"、"/proc" への着地と、広すぎる書き込み範囲を拒否する |
| 露出する組の検査で拒否する形 | 下の段と同じ実体、段を問わない祖先、生成された "hide" の中への着地、"hide" の中への "rw-copy"、"rw-copy" の中への "rw" と "rw-file" |
| 露出する組の検査で許可する形 | "hide" の再指定、間の段の "rw" が遮る形、別の根の項目への着地、"ro" の中への "rw-copy" |

### TBL-158: ビルド済みバイナリで観測する項目
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A1

| 区分 | 観測条件 |
|---|---|
| 診断、終了、実行の境界 | core-process.md と core-output.md が定める診断と終了結果の全条件。隣り合う検査段階の診断が同時に成り立つときは、先の段階の診断が出る |
| 診断、終了、実行の境界 | ファイルの読み方。FIFO でも停止せず診断になることと、読み込み量の上限の超過（core-runtime.md） |
| 診断、終了、実行の境界 | 記述子の上限。soft 上限が 1024 の環境で、1100 件の走査を行う |
| 診断、終了、実行の境界 | マウントの 6 場面を、隔離の中から読み書きして確かめる |
| 診断、終了、実行の境界 | "network.mode" の "none"、秘密が隔離の中からどう見えるか |
| 診断、終了、実行の境界 | seccomp による "TIOCSTI" の EPERM。上位ビットを立てた呼び出しと、x32 のビットを立てた呼び出しによる終了も含める |
| 診断、終了、実行の境界 | 入れ子の警告と "command not executable"。argv[0] は、隔離の中と入れ子の両方で確かめる |
| rw-copy | 表 TBL-159 の全行 |
| init と既定のプロファイル | 設定ディレクトリが無い状態から書き出せる。"~/.config" も無い場合は、そこから作る |
| init と既定のプロファイル | 書き出した内容が "examples/profile/default.toml" とバイト単位で一致する |
| init と既定のプロファイル | "secrets/" のモードが 0700 になり、既存のものも 0700 に狭まる |
| init と既定のプロファイル | 書き出し先が既存またはリンク切れなら "path" になる。"profile/" の位置が通常ファイルの場合も "path" になる |
| init と既定のプロファイル | 標準出力はパス 1 行。設定ディレクトリがリンクならリンク先へ書き、表示するのは組み立てたままのパスである |
| init と既定のプロファイル | "KAKOI=1" でも、"init" は標準エラーに何も出さず、終了コード 0 で書く |
| init と既定のプロファイル | 設定ディレクトリが通常ファイルなら "policy" になる。既定のファイルか設定ディレクトリがリンク切れの場合も "policy" になる |
| init と既定のプロファイル | 設定ディレクトリが無いとき、組み込みの既定による "--print-plan" は 0 で終わり、計画に "kakoi init" が出る。同じ状態で、追加のポリシーの "ro" に "config_dir" を使うと、その項目が飛ばしとして出る |
| init と既定のプロファイル | 設定ディレクトリが無く、存在する最も深い祖先が書き込める領域にある場合は、ポリシーを保護する配置の "path" の診断になる |
| JSON の契約 | "--print-plan" の JSON が解析でき、core-plan-output.md が定めるトップレベルのキーを持ち、秘密の値が "null" になっている |

### TBL-159: rw-copy で観測する場面
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A1

| 場面 | 隔離の中とホスト側で確かめること |
|---|---|
| 通常ファイル | ホストの内容を読める。書き込みが成功して読み戻せる。ホストの元のファイルは変わらず、終了後に残骸が無い |
| ディレクトリ | 既存の内容を読め、新規作成と上書きができる。実行ビットが引き継がれ、中で作った実行可能ファイルも実行できる。ホストのディレクトリは変わらず、残骸も無い |
| 複製元の不在と種類 | 不在なら飛ばす。根がソケットか FIFO なら "path" になる |
| 上限と複製できない内容 | エントリ数の超過は "path" になる。複製できないエントリは計画に出る |
| リンクと重なり | リンクはリンクのまま複製する。中にある狭い "rw" への書き込みはホストに届く |
| 計画 | "--print-plan" の表示と JSON の両方に、項目と指令が出る |
| 中に置いたポリシーファイル | 起動を許可する。隔離の中で書き換えても、ホストには出ない |

## Examples

```gherkin
@id=EX-648 @about=REQ-354 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 計画算出の純粋な境界・成功
  Given 合成した事実を計画算出に渡す
  When 契約への適合を確認する
  Then 外部I/Oなしで記号付き引数・環境・適用結果・コマンドを比較できる

@id=EX-649 @about=REQ-354 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 計画算出の純粋な境界・反例
  Given 合成した事実を計画算出に渡す
  When 契約への適合を確認する
  Then 計画算出がホストのファイルを直接読むことは契約違反である

@id=EX-650 @about=REQ-355 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1,docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A1
Scenario: 純粋関数の検証範囲・成功
  Given TBL-156 と TBL-157 の各行とテストを照合する
  When 契約への適合を確認する
  Then 列挙した成立・不成立の条件をテストで確認できる

@id=EX-651 @about=REQ-355 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1,docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A1
Scenario: 純粋関数の検証範囲・反例
  Given TBL-156 と TBL-157 の各行とテストを照合する
  When 契約への適合を確認する
  Then 根の検査の拒否だけ確認し対象外のroを確認しないことは契約違反である

@id=EX-652 @about=REQ-356 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 実バイナリの検証範囲・成功
  Given 実バイナリのテストを実行する
  When 契約への適合を確認する
  Then 隔離内外の観測を伴う検証がネットワークなしで通る

@id=EX-653 @about=REQ-356 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 実バイナリの検証範囲・反例
  Given 実バイナリのテストを実行する
  When 契約への適合を確認する
  Then 純粋関数だけの成功で実バイナリを検証済みにすることは契約違反である

@id=EX-654 @about=REQ-357 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 移行時の一度限りの確認・成功
  Given 旧jailから移行する
  When 契約への適合を確認する
  Then 利用者が実機のマウント集合を比較する

@id=EX-655 @about=REQ-357 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 移行時の一度限りの確認・反例
  Given 旧jailから移行する
  When 契約への適合を確認する
  Then 旧スクリプトを製品の恒久テスト依存にすることは契約違反である

```
