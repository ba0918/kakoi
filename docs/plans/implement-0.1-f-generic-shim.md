# process-wrap 0.1 実装計画 F: シムを汎用の本体とツール節にし、既定を全部隔離にする（仕様改訂 dec21ed）

この計画は、計画 A〜E で実装した 0.1 に、仕様の改訂（コミット dec21ed、`docs/spec/process-wrap.md`）を
反映する。全体像 `docs/plans/implement-0.1.md` の「Approach and why」「Left to the implementer」
「Stop conditions」「Test command」はそのまま前提にする。改訂で変わったのは 2 節（「シム」の定義、
「モデルが動かない」の追加、「セットアップスキル」の定義）、16 節（シムの雛形の段落、セットアップ
スキルの段落、README のインストールの節への要求）、18 節（codex 以外のツール節の値）、19 節（却下した
案の追加）、20 節（未決の追加）である。実装者は先に全体像と、仕様の 2 節・16 節・18 節・20 節と、用語集
`CONTEXT.md` の「シム」「モデルが動かない」「セットアップスキル」を読むこと。製品のコード（`src/`、
`tests/`）は変えない。ステップ番号は全体像から続けて 33〜38。

## Goal

同梱のシムの雛形が、包む対象に依らない本体と先頭のツール節から成り、既定ですべての起動を
`process-wrap` に通し、素通しは利用者が承認した許可リストだけになっていて、README と
セットアップスキルがその形を説明している `process-wrap` 0.1。

## Specification

`docs/spec/process-wrap.md`（コミット dec21ed で改訂。承認済み）。用語は `CONTEXT.md` に従う。

## Approach and why

雛形は差分で直さず、書き直す。現在の雛形（main 33f30e2）はサブコマンドを 3 つの一覧で分類して隔離の
要否とフラグの位置を決める形で、仕様 19 節がこの方式を却下した。分類の関数と「値を取るオプションの
一覧」を消し、次の形にする。

- **先頭のツール節。** 包む対象ごとに書き換える値だけを、ファイルの先頭に bash の変数と配列で置く。
  仕様 16 節の 5 つ: 実体のコマンド名、差し込むフラグ（空を許す）、引数の写しの対応（`--workspace` に
  写すオプション名と `--rw` に写すオプション名。表し方は実装者）、素通しの許可リスト（空）、フラグを
  付けない一覧（空）。同梱するのは codex の値（`codex`、`--dangerously-bypass-approvals-and-sandbox`、
  `--cd` と `-C` を `--workspace` に、`--add-dir` を `--rw` に）。ツール節の終わりに「ここから下は
  書き換えない」の区切りを置く（仕様に無い、この計画の具体化）。
- **汎用の本体。** 順に: `PROCESS_WRAP_SHIM_OFF=1` なら受け取った引数列をそのまま実体に渡して exec。
  引数列の先頭の語が許可リストにあれば同じく素通し（受け取った引数列のまま）。素通しの経路では
  `/tmp/process-wrap` を作らない。先頭の語がフラグを付けない一覧にあればフラグ無し、それ以外はフラグを
  引数列の先頭に置く（フラグが空なら何も置かない）。引数を走査して写しの対応に従い `--workspace` と
  `--rw` を組み立てる（値は次の語、`=` で付けた形、短いオプションに続けた形。`--` より後ろは走査
  しない）。写しが無ければ `--workspace` にはカレントディレクトリを渡す（常に付ける。現在の雛形と
  同じ）。写し先の存在は検査しない。`/tmp/process-wrap` がシンボリックリンクなら止まり、無ければ作る。
  `PATH` から自分自身を除いて実体を探し、解決した絶対パスを `COMMAND` として `process-wrap` を exec
  する。`--help` や `--version` の特別扱いは無い。実行時にヒントは出さない。
- **ヘッダーのコメント。** 仕様 16 節が雛形に持たせるもの 2 つ: 壊れ方と足す先の表（3 行）、許可
  リストに最初に入りそうな例（認証のサブコマンド。codex なら `login`。実測していない）。加えて、この
  計画の具体化として 2 つ: 確かめ方（`PATH` の先頭に引数を表示するだけの `process-wrap` の代わりを
  置く）、ツール節の埋め方。

README とスキルは、仕様 16 節の要求と 1 対 1 で対応させて書き直す。README の「Wrap codex」の箇条は
「Wrap a command」にし、同梱の値が codex の例であることを先に言う。スキルの手順 4（`codex --help` と
分類の突き合わせ）は無くなり、代わりに「隔離の中で壊れたコマンドの扱い」と「ツール節の値の提案
（対象の `--help` だけを根拠に、実測していないと明記）」が入る。

雛形・README・スキルは仕様 16 節が「人が確かめる」と定め、第 15 節のテストの対象にしない。この計画は
テストを 1 つも足さず、`cargo test` の結果は変わらない。各ステップの完了は check（コマンドと
突き合わせ）で示す。雛形の観測の仕掛けは、scratchpad の下にディレクトリを 2 つ作る。前のディレクトリに
雛形の写し（`SHARED_DIR` だけ scratchpad の下に向けたもの）を `codex` の名前で置き、後のディレクトリに
「受け取った引数を 1 行ずつ表示して 0 で終わる」だけの `codex` と `process-wrap` の代わりを置き、
`PATH` を前・後の順にして `codex ...` を起動する。雛形は自分自身を除いて後の `codex` を実体として
見つける。本物の `/tmp/process-wrap` は触らない。本物の codex は起動しない（ステップ 37 と 38 だけが
人の手で本物を起動する）。

## Scope of change

- `examples/shim/codex`（書き直し）、`README.md`（シムの段落、「Not in 0.1」、Known gaps の 9 の近くの
  `/tmp/process-wrap` の記述があれば確認）、`skills/process-wrap-setup/SKILL.md`（手順と description）、
  `CHANGELOG.md`（0.1.0 のシムとスキルの項目）

変更しないもの: `src/`、`tests/`、`docs/spec/`、`CONTEXT.md`、`docs/plans/`、`examples/profile/`、
`lefthook.yml`、`.claude/`、`Cargo.toml`、`Cargo.lock`、`PROJECT.md`。

## Step order and prerequisites

33 → 34 → 35 → 36 → 37 → 38。33 は雛形、34 は README、35 はスキル、36 は CHANGELOG、37 と 38 は人の
手による実機の観測。前提は main が dec21ed 以降、開発機に `bash` と `shellcheck`。仕様 16 節が名指す
Agent Skills の検証ツール `skills-ref validate` は、この開発機では同じパッケージが `agentskills` の名前で
入っている（2026-09-08 に `agentskills validate` で確認）。無い開発機では計画 E と同じく frontmatter を
`rg` で確かめて報告する。

## Verification map（この計画の分）

| 仕様の節（改訂で変わった規則） | ステップ |
|---|---|
| 2 シム、2 モデルが動かない、16 公開ドキュメント（シムの雛形の段落: ツール節 5 つ、既定は全部隔離、例外 2 つ、本体の動き、照合は先頭の語、表、例、確かめる条件） | 33 |
| 16（README のインストールの節への要求: コマンドを包む手順、同梱の値が codex の例、表、最初に入りそうな例、確かめる条件）、18（codex 以外のツール節の値） | 34 |
| 2 セットアップスキル、16（セットアップスキルの段落: すること 5 つ、守ること 4 つ、隔離の外で走る）、20（エージェント CLI でない包む対象の手順は未決） | 35 |
| 17 版とリリース（CHANGELOG） | 36 |
| 16（保守側の確かめる条件のうち「同梱の値で先頭に置いたフラグが対象に効く」） | 37 |
| 16（セットアップスキルの人が確かめる条件のうち「1 回の試行で書き込みの前に差分と承認の要求が出る」） | 38 |

## Left to the implementer（この計画の分）

- ツール節の変数と配列の名前、写しの対応の表し方（2 つの配列か、1 つの連想配列か）、区切りのコメントの
  文言。
- 本体の関数の分け方と、引数の走査の書き方（既存の雛形の走査の構造を流用してよい。`--` の扱いと 3 つの
  値の形は仕様 16 節が固定する）。
- ヘッダーのコメントの英文と並び。README・SKILL.md・CHANGELOG の英文と構成。

## Stop conditions（この計画に固有）

- 先頭の語だけと照合する規則で、既存の雛形の振る舞いのうち仕様に無いものを保つ必要が出た（例:
  `--help` が先頭にあるときの素通し）。仕様 16 節が「例外にしない」と決めているので保たない。保つ
  理由があると思ったら止める。
- 写しの取りこぼし（3 つの形で読めないオプション。複数の値を取る `-i` など）が、隔離が緩む向きに
  働くと分かった。仕様 16 節は「緩む向きに働かない」を前提に取りこぼしを受け入れているので、前提が
  崩れたら止める。緩まない向きの取りこぼしは、ヘッダーに書いて進む。

## Test command

この計画はテストを足さない。ステップ 33〜35 の check のコマンドは各ステップに書く。ステップ 33 の前に
PROJECT.md の検査 4 本を順に実行して合格数を控え、ステップ 36 のあとにもう一度実行して、合格数が同じで
失敗 0、fmt・clippy 通過であることを報告に書く。

## Out of scope

opencode・claude のツール節の値（仕様 18 節）、製品のコードの変更、同梱プロファイルの `ro ~/.codex/skills`
の見直し（20 節の未決）、エージェント CLI でない包む対象に固有のスキルの手順（20 節の未決）、dotfiles 側の
シムの置き換え（利用者の作業）、0.1.0 の tag。

---

## Step 33 — シムの雛形を汎用の本体とツール節に書き直す

Purpose: `examples/shim/codex` を、包む対象に依らない本体と先頭のツール節から成り、既定ですべての起動を
`process-wrap` に通す形にする。
Specification: 仕様#2 シム、仕様#2 モデルが動かない、仕様#16 公開ドキュメント（シムの雛形の段落。
ツール節の 5 つ、既定と例外 2 つ、本体の動き、フラグの位置と一覧、写しの読み方、照合は先頭の語だけ、
壊れ方と足す先の表、最初に入りそうな例、確かめる条件）、仕様#18 0.1 で作らないもの（codex 以外の
ツール節の値）、仕様#19 却下した代替案（分類する方式、ヒントを出す案、`--help` の特別扱い）。
Prerequisites: main が dec21ed 以降。開発機に `bash` と `shellcheck`。
May change: `examples/shim/codex`。
Done when: 雛形が Approach の形になっていて、`examples/shim/codex` が実行可能で、`bash -n` と
`shellcheck` が通り、Approach の仕掛け（前に雛形の写し、後に `codex` と `process-wrap` の代わり）で次の
観測がすべて期待どおり（本物の `/tmp/process-wrap` は触らない。本物の codex は起動しない）。

1. 引数無しの `codex`、`codex exec hi`、`codex -m x exec hi`、`codex hi`、`codex --help`、
   `codex --version`、`codex login`（許可リストが空のとき）のどれでも `process-wrap` の代わりが起動され、
   `--` の直後の `COMMAND` が後のディレクトリの `codex` の絶対パスで、`codex` の引数列の先頭に
   `--dangerously-bypass-approvals-and-sandbox` があり、その後ろは受け取った引数のまま。
2. `codex --cd /w exec hi`、`codex -C /w exec hi`、`codex -C=/w exec hi`、`codex -C/w exec hi`、
   `codex --cd=/w exec hi` のどれでも `process-wrap` の引数列に `--workspace /w` が出る。
   `codex --add-dir /a --add-dir=/b exec hi` で `--rw /a` と `--rw /b` が出る。
   `codex exec -- --cd /w` では `--workspace` がカレントディレクトリのまま（`--` より後ろは走査しない）。
3. 許可リストに `login` を一時的に入れた写しで、`codex login` は `process-wrap` の代わりが起動されず
   `codex` の代わりが受け取ったままの引数で起動され、`SHARED_DIR` が無い状態なら作られない。
   `codex -m x login` は 1 と同じく隔離に入る。
4. フラグを付けない一覧に `review` を一時的に入れた写しで、`codex review --base x` は `process-wrap` の
   代わりが起動され、`codex` の引数列にフラグが無い。`codex -m x review` は 1 と同じくフラグが付く。
5. ツール節のフラグを空にした写しで、`codex exec hi` は `process-wrap` の代わりが起動され、`codex` の
   引数列は受け取ったまま。
6. `PROCESS_WRAP_SHIM_OFF=1 codex exec hi` は `process-wrap` の代わりが起動されず、`codex` の代わりが
   受け取ったままの引数で起動され、`SHARED_DIR` が無い状態なら作られない。
7. `SHARED_DIR` をシンボリックリンクにした写しで `codex exec hi` は 0 以外で終わり `process-wrap` の
   代わりが起動されない。無い状態では作られる。

ヘッダーのコメントに、壊れ方と足す先の表（3 行）、最初に入りそうな例（認証。codex なら `login`。実測
していない読み方の例）、確かめ方（`process-wrap` の代わりを `PATH` の先頭に置く）、ツール節の埋め方が
ある。無いもの 4 つ: 分類の関数（`is_isolated_subcommand` など）、「値を取るオプションの一覧」、写し先の
存在の検査、実行時のヒントの出力。
Shown by: check — `test -x examples/shim/codex`、`bash -n examples/shim/codex`、
`shellcheck examples/shim/codex`、上の観測 1〜7 を写しで実行し、報告に実際の引数列を書く。ヘッダーの
項目 4 つと「無いもの」4 つを行番号と突き合わせ、対応の無い項目が 0。
Left to the implementer: 変数と配列の名前、写しの対応の表し方、走査の書き方、コメントの英文。
Stop and hand back if: 3 つの値の形で `--workspace` の値が一通りに読めない対象のオプションが codex に
あり、それを写さないと隔離が緩む向きに働くと分かった（仕様は緩まない向きだけを受け入れている）。

## Step 34 — README のシムの段落を「Wrap a command」に書き直す

Purpose: 仕様 16 節が README のインストールの節に定めた、シムの雛形の見せ方を書く。
Specification: 仕様#16 公開ドキュメント（README のインストールの節への要求のうちシムの項、シムの雛形の
段落の末尾「README のシムの段落には…を載せる」）、仕様#18 0.1 で作らないもの（codex 以外のツール節の
値）。
Prerequisites: ステップ 33（雛形の実物を指して書く）。
May change: `README.md`。
Done when: README の「Wrap codex」の箇条が「Wrap a command」になり、次を含む: 雛形を写して先頭の
ツール節を埋めて `PATH` に置く手順、同梱の値が codex の例であること（`process-wrap` は包む対象に
依らない）、既定は全部隔離で素通しは利用者が承認した許可リストだけ（ホワイトリスト方式、自己責任）、
壊れ方と足す先の表（雛形と同じ 3 行）、最初に入りそうな例（認証）、保守側と利用者の確かめる条件、
利用者の条件に要る「モデルが動かない」の意味（仕様 2 節）とその確かめ方（対象の文書を読み、起動して
観測する。codex なら起動時のヘッダーと `exec` の表示）、`PROCESS_WRAP_SHIM_OFF=1`。「Not in 0.1」の「no shims for claude or opencode」の箇条が「codex 以外の
ツール節の値は同梱しない」の趣旨になる。分類・一覧の数・`codex --help` との突き合わせの記述が無い。
Shown by: check — 仕様 16 節の README への要求（シムの項の各要素）を 1 項目ずつ README の段落と
突き合わせ、対応の無い項目が 0 であることを報告に書く。`rg -n 'Wrap a command' README.md`（1 行以上）、
`rg -n 'PROCESS_WRAP_SHIM_OFF' README.md`（1 行以上）、`rg -c 'classif' README.md` が 0、
`rg -c 'three lists|two lists' README.md` が 0。
Left to the implementer: 英文と構成。
Stop and hand back if: なし。

## Step 35 — セットアップスキルの手順を仕様の 5 つに書き直す

Purpose: `skills/process-wrap-setup/SKILL.md` の「What to do」を仕様 16 節の 5 つにし、分類の突き合わせを
消し、対象をエージェント CLI に限らない言い方にする。
Specification: 仕様#2 セットアップスキル、仕様#16 公開ドキュメント（セットアップスキルの段落: すること
5 つ、守ること 4 つ、隔離の外で走る、人が確かめる条件）、仕様#20 未決と委譲（エージェント CLI でない
包む対象の手順は未決）。
Prerequisites: ステップ 33（雛形のツール節と表を指して書く）。
May change: `skills/process-wrap-setup/SKILL.md`。
Done when: 「What to do」が 5 つ: (1) 入っている包む対象のコマンド（エージェント CLI を含む）を確かめて
プロファイルに足す項目（対象が書く状態の置き場の `rw`、対象が読む設定や指示ファイルの `ro`）を提案、
(2) 雛形を写して先頭のツール節を埋めた設置を提案（ツール節の値は対象の `--help` だけを根拠に、実測して
いないと明記。実測は利用者）、(3) 代替コマンドの `path-prepend`、(4) 隔離の中で壊れたコマンドを持ち
込まれたら、雛形の表に従ってどちらの一覧に足すかまたはプロファイルで直すかを差分で提案（判断は
利用者）、(5) 前後の `--print-plan` の並置。「What to keep to」4 つはそのまま。「隔離の外で走る」は
そのまま。分類・`codex --help` との突き合わせ・一覧の数の記述が無い。`description` が新しい手順に合う。
Agent Skills の検証ツール（仕様 16 節の `skills-ref validate`。この開発機では `agentskills validate`）が
あれば通る。
Shown by: check — 仕様 16 節の「すること」5 つと「守ること」4 つと「隔離の外で走る」を SKILL.md の
該当行と 1 つずつ突き合わせ、対応の無い項目が 0 であることを報告に書く。
`rg -c 'classif|is_isolated|is_passed' skills/process-wrap-setup/SKILL.md` が 0。
`agentskills validate skills/process-wrap-setup`（無ければ frontmatter の `name` と `description` を `rg` で
確かめ、検証ツールは利用者が後で走らせると報告する）。仕様の 2 つ目の人が確かめる条件（1 回の試行で
書き込みの前に差分の提示と承認の要求が出る）はステップ 38。
Left to the implementer: 英文と構成。
Stop and hand back if: なし。

## Step 36 — CHANGELOG のシムとスキルの項目を直す

Purpose: 未リリースの 0.1.0 の項目を、改訂後の仕様の形（既定は全部隔離、汎用の本体とツール節、
許可リスト）に合わせる。
Specification: 仕様#17 版とリリース、仕様#16 公開ドキュメント（シムの雛形の段落）。
Prerequisites: ステップ 35。
May change: `CHANGELOG.md`。
Done when: 0.1.0 のシムの項目が「classifies codex invocations」でなく、既定で全部を `process-wrap` に
通し素通しは許可リストだけで、雛形が汎用の本体とツール節から成る旨になり、スキルの項目が新しい
手順（壊れたコマンドの扱いを含む）に合う。新しい項目は足さない（0.1.0 は未リリース）。
Shown by: check — `rg -c 'classif' CHANGELOG.md` が 0。`rg -n 'allow' CHANGELOG.md`（1 行以上。許可リストの
語）。そのあと PROJECT.md の検査 4 本を順に実行し、ステップ 33 の前に控えた合格数と同じで失敗 0、fmt
差分なし、clippy 警告なし。
Left to the implementer: 英文。
Stop and hand back if: なし。

## Step 37 — 同梱の値で、先頭に置いたフラグが codex に効くことを人が確かめる

Purpose: 仕様 16 節の保守側の確かめる条件のうち、本物の対象を起動しないと分からない 1 つを、利用者が
自分の機械で確かめる。
Specification: 仕様#16 公開ドキュメント（シムの雛形の段落の「保守側が同梱の雛形について確かめる
条件」の最後の 1 つ）。
Prerequisites: ステップ 33。利用者の機械に codex（0.153.4 で実測した経緯がある）と、書き直した雛形を
写した `PATH` の先頭のシム。
May change: なし。
Done when: 利用者が雛形経由で `codex review --base <ref>` を起動し、codex が表示するヘッダーの `sandbox:`
が `read-only` 以外である。
Shown by: external — 利用者が自分の機械で実行し、ヘッダーの行を報告する。本物の codex を起動しモデルに
問い合わせるため、実装者は行わず、人が行う。cycle の終端報告で「人が確かめる残件」として渡す。
Left to the implementer: なし。
Stop and hand back if: `read-only` のままだった（仕様 16 節の保守側の条件が同梱の値で満たされない。
同梱の一覧は空のままにするか、フラグの位置の規則を変えるかは仕様の判断なので、仕様に戻す。利用者は
それまで自分の写しで表の 1 行目に従える）。

## Step 38 — セットアップスキルの 1 回の試行で、書き込みの前に差分と承認の要求が出ることを人が確かめる

Purpose: 仕様 16 節のセットアップスキルの人が確かめる条件のうち、エージェント CLI を実際に走らせないと
分からない 1 つを、利用者が自分の機械で確かめる。
Specification: 仕様#16 公開ドキュメント（セットアップスキルの段落の「人が確かめる条件」の 2 つ目）。
Prerequisites: ステップ 35。利用者の機械にエージェント CLI と、入れたスキル。
May change: なし。
Done when: 利用者が隔離の外でスキルを 1 回試行し、最初の書き込みの前に差分の提示と承認の要求が出る。
Shown by: external — 利用者が自分の機械で実行し、結果を報告する。エージェント CLI を起動しモデルに
問い合わせるため、実装者は行わず、人が行う。cycle の終端報告で「人が確かめる残件」として渡す。
Left to the implementer: なし。
Stop and hand back if: なし（実装者はこのステップを行わない）。
