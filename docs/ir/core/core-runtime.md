# 実行時のI/Oと依存

既存仕様から継承した本体の検査用表現。同じ責務を一文書にまとめ、見出しと要求IDで参照する。この IR が正本である。

## Requirements

### REQ-305: ホストへの書込み境界
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

コマンドを包む起動は計画表示を含め永続状態を持たず、自身でホストのファイルを書かない。initだけが設定ディレクトリまでの欠けた成分とprofile/、secrets/、出力ファイルを作る。/tmp/kakoiを作らない。

### REQ-306: 収集する事実
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

本体はポリシー・秘密、.gitとGit関係ファイル、共通ディレクトリHEADの存在種類、マウント一覧、走査ディレクトリ、候補パスの存在種類実体、配置検査の通過ディレクトリとリンク先、サブモジュールconfig、適用rw-copyの名前種類モード内容リンク先を読む。HEAD内容は読まない。filtered追加観測は既存のネットワーク仕様に従う。

### REQ-307: 外部コマンド
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

host/noneはbwrap以外の外部コマンドを実行せず、bwrapをホストPATHから探し不在ならbwrap診断にする。filtered追加依存は既存の実証完了条件で確定する。

### REQ-308: メモリ上の記述子
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

hideの空ファイル、rw-copy各通常ファイル、seccompをメモリ上のファイル記述子でbwrapへ渡しホストのファイルシステムに残さない。記述子を用意できない場合はbwrapで125。番号は起動直前に割り当て計画では記号を示す。

### REQ-309: ファイル数上限
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

記述子を作る前に開けるファイル数のsoft上限を毎回hard上限まで上げ、コマンドに継承する。入れ子では記述子を作らず上限を上げない。rw-copyは通常ファイル1個につき記述子1個を使う。

### REQ-310: 通常ファイルだけを読む
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

ポリシーファイル・秘密ファイル・.gitとG配下の読み取るファイル・rw-copy複製元のファイルは待たずに開き、開いた記述子で通常ファイルと確認する。FIFOで停止しない。ポリシー違反はpolicy、秘密はsecret、Gitとコピーはpathとする。

### REQ-311: 読込量の上限
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

ポリシーと.git関連の通常ファイルは1ファイル1MiBを上限とし超過を非通常ファイル同様に拒否する。秘密は改行除去後64KiBが先に効く。rw-copyは1MiBでなく項目あたり4096エントリ・通常ファイル内容合計64MiBで判定する。マウント一覧と走査ディレクトリはこの読込規則の対象外。

### REQ-312: 読むリンクの区別
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

ポリシーと秘密はリンクを辿り配置保護で実体を検査する。.gitとG配下、および実体化済みrw-copy複製元の最後の成分がリンクなら辿らない。複製元ディレクトリ内のリンクはリンクとして複製する。

### REQ-313: パス解決のリンク数
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

配置検査のパス解決はリンク40本まで辿り、超過は不在成分と同じ扱いにする。保護対象はそこまでの参照を検査し、書かれた項目は実体なしとして飛ばし、workspaceはpath診断とする。

### REQ-314: 信頼する起動環境
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

HOME、XDG_CONFIG_HOME、PATH、KAKOI、cwdを信頼し、ホストでの操作がプロファイル・入れ子判定・作業場所を変えることをREADMEに記す。cgroup資源制限は対象外。

### REQ-315: 固定引数の順序
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

host/noneの固定引数は--ro-bind / /、--dev /dev、--proc /proc、--unshare-all、hostだけ--share-net、--die-with-parent、--chdir cwd、--seccomp記述子、--argv0 COMMANDの順。続けて適用順のマウント、1個の--、解決済みコマンドパス、ARGSを置く。ポリシーで固定部分を変えない。

### REQ-316: COMMANDなしの固定引数
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

計画表示でCOMMAND省略時は--argv0とその値、末尾の--とその後を引数列から省く。filteredの接続機構と起動順序は既存の実証工程で確認する。

### REQ-437: マウント一覧のパスをバイト列として扱う
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-last-four.md#A4
- verification: unit

マウント一覧を読むとき、各項目のマウント先のパスはバイト列として扱い、UTF-8として読めない名前の項目も落とさずに一覧に残す。

## Examples

```gherkin
@id=EX-538 @about=REQ-305 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: ホストへの書込み境界
  Given 本体の既存仕様を適用する
  When 計画表示を2回行う
  Then 前回結果を保存せず同じ入力と事実から同じ計画を得る

@id=EX-539 @about=REQ-306 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 収集する事実
  Given 本体の既存仕様を適用する
  When linked worktreeの共通ディレクトリを調べる
  Then HEADの存在と通常ファイルであることを調べる

@id=EX-540 @about=REQ-307 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 外部コマンド
  Given 本体の既存仕様を適用する
  When ホストPATHにbwrapがない
  Then コマンド解決より先にbwrap診断を出す

@id=EX-541 @about=REQ-308 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: メモリ上の記述子
  Given 本体の既存仕様を適用する
  When 全量計画を起動前に表示する
  Then 記述子番号ではなく記号を示す

@id=EX-542 @about=REQ-309 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: ファイル数上限
  Given 本体の既存仕様を適用する
  When soft上限1024で1100個のhideファイルを使う
  Then hard上限が足りれば起動しコマンドへ引き上げ後の値を渡す

```

```gherkin
@id=EX-543 @about=REQ-310 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 通常ファイルだけを読む
  Given 本体の既存仕様を適用する
  When policy-fileにFIFOを指定する
  Then 待たずにpolicyで125となる

@id=EX-544 @about=REQ-311 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 読込量の上限
  Given 本体の既存仕様を適用する
  When ポリシーファイルが1MiBを1バイト超える
  Then policyで125となる

@id=EX-545 @about=REQ-312 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 読むリンクの区別
  Given 本体の既存仕様を適用する
  When 秘密ファイルが通常ファイルへのリンクである
  Then リンク先の値を読む

@id=EX-546 @about=REQ-313 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: パス解決のリンク数
  Given 本体の既存仕様を適用する
  When 書かれたマウント項目がリンク上限を超える
  Then 実体を持たない項目として飛ばす

@id=EX-547 @about=REQ-314 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 信頼する起動環境
  Given 本体の既存仕様を適用する
  When ホストでKAKOI=1を与える
  Then 入れ子として扱う

```

```gherkin
@id=EX-548 @about=REQ-315 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 固定引数の順序
  Given 本体の既存仕様を適用する
  When COMMANDが-で始まる相対パスである
  Then --の後のコマンドとして渡す

@id=EX-549 @about=REQ-316 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: COMMANDなしの固定引数
  Given 本体の既存仕様を適用する
  When COMMANDなしで計画を作る
  Then argv0指定もコマンド用末尾区切りもない

```
