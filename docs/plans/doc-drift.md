# doc-drift 実装計画

## 目的

承認済みの 0.2 仕様と実装に対して残っている文書のずれを、対象の 8 ファイルの記述を直して解消する。コード、テスト、雛形、仕様は変えない。

## 仕様

`docs/spec/kakoi.md`（`fb8fc09` 時点、`Status: Approved`）。各手順が参照する節をその都度示す。この計画は各手順に「意味」を固定するが、仕様の本文は写さない。意味の典拠は常にこの仕様であり、実装者は示された節を読んで確かめる。

## 進め方とその理由

- 変更は文書だけ。このリポジトリは `PROJECT.md` で `cargo build --locked`、`cargo test --all-targets --locked`、`cargo fmt --check`、`cargo clippy --all-targets --locked -- -D warnings` を検査として定めているが、いずれも文書だけの変更を検証しないので、この計画では実行しない。文書にテストは無いので、テストも足さない。
- 証拠は静的検査（古い主張が消えたことを見る `rg`、固定した意味の文が入ったことを見る `rg`、`git diff --check`）と、その検査が守る意味を与える仕様の節。検査は手順ごとに書く。
- 対象は下に挙げる閉じたリストだけ。修正は主張の文単位にとどめ、文書全体を書き直さない。差分をレビューしやすく保つため。
- 意味は仕様と実装が決める。文言・語順・段落構成は実装者が選ぶ。仕様の判断が要る事項はこの計画に無い。
- 人間の判断が要るのは、この計画の承認と、出来上がった差分の受け入れだけ。途中に人間の承認が要る不可逆・特権・危険な操作は無い。

## 変更する範囲

- 変更してよいファイルは次の 8 つだけ。手順ごとの「May change」が範囲をさらに狭める。
  - `README.md`
  - `docs/cli.md`
  - `docs/getting-started.md`
  - `docs/policy.md`
  - `docs/security.md`
  - `docs/shim.md`
  - `PROJECT.md`
  - `CONTEXT.md`
- 成果物は 1 つ: ブランチ `doc-drift` の 1 つの変更セット。保存する状態は無い。
- 新しい依存は増やさない。手順 7 は開発環境に既にある `run-if-present`（mise で入れる `github:ba0918/run-if-present`）を `PROJECT.md` に説明するだけで、導入は変えない。
- 既に直っていて触らない箇所:
  - `README.md:166-167` と `docs/security.md:32-34`（rw-copy の合成で「書き込みがホストに残るのは `rw` と `rw-file` だけ」に修正済み）
  - `docs/spec/kakoi.md:3`（`fb8fc09` で `Status: Approved`。`PROJECT.md` の「承認済み仕様」という参照はこれで整合済み）
  - `docs/getting-started.md:33-37`（ユーザー名前空間の段落は仕様 3 のもとで正確。実装者は触らない）
- 行番号は `fb8fc09` 時点の目印。手順は引用した現在の文で箇所を探す。番号がずれていても、同じ主張の文を探す。

## 手順の順序と前提

- 全体の前提: 開始時の作業ツリーがクリーンで、`docs/spec/kakoi.md` が `Status: Approved`、変更対象の 8 ファイルがこの計画の引用する文を含むこと（行番号は `fb8fc09` 時点の目印）。
- 8 つの手順は互いに独立で、順序に依存しない。どれか 1 つを終えるたびに、その手順の検査だけを実行して完了を確かめられる。
- 手順 1 は `README.md` と `docs/policy.md`、手順 2 は `README.md` と `docs/security.md` に、1 つの意味の修正を同時に行う。ほかは 1 手順 1 ファイル。
- 読み取りだけするファイル: 手順 1 は `examples/profile/default.toml`、手順 7 は `tests/launch.rs`・`tests/cli.rs`・`lefthook.yml`、手順 8 は `src/workspace_facts.rs`。いずれも変更しない。

## 検証の対応

| 手順 | 仕様の節 | 固定する意味 |
|---|---|---|
| 1 | 2 節「用語」（段、組み込みの既定）、5.3 節「段の合成」、6.3 節「生成される項目」、16 節「公開ドキュメント」 | 書かれた 3 段の上の生成の段と、生成の項目による置き換え。同梱プロファイルが実際に使う節 |
| 2 | 2 節「用語」（隔離）、3 節「対応環境」 | 名前空間の断定の条件 |
| 3 | 4.1 節「文法」（`init`）、13 節「出力と終了コードの契約」 | `init` が作るディレクトリのモード。`--print-plan=full` に含まれるもの |
| 4 | 4.2 節「コマンドの解決」、12.1 節「入れ子」、13 節「出力と終了コードの契約」 | 入れ子 126 の例。`--print-plan` 付き入れ子の振る舞い。組み込みの既定の表示。出所の注記 |
| 5 | 16 節「公開ドキュメント」 | シムが `kakoi` に渡す引数と、先頭に差し込むフラグ |
| 6 | 6.2 節「実体への解決と存在しないパス」、6.3 節「生成される項目」、7 節「ネットワーク」 | 生成項目の走査の対象と、`none` の残すもの |
| 7 | 14 節「実行時の境界」、15 節「検証の契約」（15.2 節「ビルド済みバイナリのテスト」）、`lefthook.yml` | テストが起動するコマンドの形。hook の前提。`init` だけが書くという例外 |
| 8 | 2 節「用語」（プロファイル、ワークツリー、ホームディレクトリ）、13 節「出力と終了コードの契約」 | `CONTEXT.md` の 3 語 |

## 実装者に委ねる点（計画全体）

- 各ファイルの文言、語順、段落の作り直し。ただし各手順の「固定する意味」を変えない。
- 検査コマンドの細部（`rg` パターンの綴り、複数の検索をまとめるか）。ただし「古い主張が無い」と「固定した意味の文がある」の両方を確かめること。
- コミットの切り方は実装フェーズの規約に従う（この計画はまとめ方を指定しない）。

## 停止条件（計画全体）

- `docs/spec/kakoi.md` が `Status: Approved` でない、またはこの計画が引用した現在の文が対象ファイルに見つからない: この計画の意味の対応と検査の前提が動いているので、手を止めて報告する。
- 参照する仕様の節見出しが無い、または手順が固定した意味と仕様が食い違う: 仕様を直すか計画を直すかは仕様の判断なので、ここでは決めずに止める。
- 文書を直すためにコード、テスト、雛形、`docs/spec/kakoi.md` を変える必要が出た: この計画の範囲外なので止める。
- 実装が仕様と違うことを見つけた（例: 手順 2 で cgroup 名前空間を常に切っている）: 文書を実装に寄せず、止めて報告する。

## 手順

### 手順 1 — README の段と同梱プロファイルの説明を直す

Purpose: README の「3 層だけが合成される」という主張を、仕様の段（書かれた 3 段とその上の生成の段）に合わせて直し、同梱の `examples/profile/default.toml` が「全節を使う」という主張（README と `docs/policy.md` の 2 箇所）を、そのファイルが実際に使う節に合わせる。
Specification: `docs/spec/kakoi.md` 2 節「用語」（段、組み込みの既定）、5.3 節「段の合成」、6.3 節「生成される項目」、16 節「公開ドキュメント」。
Prerequisites: なし。`examples/profile/default.toml` を読めること（読み取りだけ）。
May change: `README.md`、`docs/policy.md`。
Done when:
- 「Up to three layers are merged」と読める文が無く、段の説明に、書かれた 3 段の上に生成の段があり、生成の段の項目が同じ実体の書かれた項目を置き換えることが書かれている。
- 「shows every section in use」（README）と「shows all of it in use」（`docs/policy.md`）と読める文が無く、同梱ファイルの説明が、そのファイルに無い節（`rw-copy`、`[git.instead-of]`、`env.pass`、`env.set`、`env.path-prepend`）を使っていると読めない。同梱ファイルが実際に使うのは `mounts`（`rw`・`rw-file`・`ro`・`hide`・`[[mounts.scan]]`・`[[mounts.hide-mounts]]`）、`network`（`mode`）、`env`（`mode`・`unset`）、コメントだけの `secrets`。
Shown by: check —
```sh
rg -n 'Up to three layers' README.md    # 何も出ない
rg -n 'shows every section' README.md   # 何も出ない
rg -n 'shows all of it' docs/policy.md  # 何も出ない
rg -n 'generated' README.md             # 生成の段に触れる行が出る。読み、置き換えの意味を確かめる
git diff --check -- README.md docs/policy.md
```
Left to the implementer: 段の説明の文と箇条書きの構成、同梱ファイルの説明の文面（README と docs/policy.md の両方）。書かれた 3 段を "layers" と呼び続けるかどうかは意味が変わらない範囲で選んでよい。
Stop and hand back if: 段の数え方そのものを変える必要が出た（仕様の段の定義と README の構成が両立しない）なら止める。

### 手順 2 — README と security.md の名前空間の断定を直す

Purpose: 「プロセス ID、IPC、UTS、cgroup、ユーザーの各名前空間を常に切る」という断定を、仕様 2 節と 3 節の条件付きの記述に直す。
Specification: `docs/spec/kakoi.md` 2 節「用語」（隔離）、3 節「対応環境」。
Prerequisites: なし。
May change: `README.md`、`docs/security.md`。
Done when: 両ファイルから「cgroup, and user namespaces are always unshared」という断定が消え、代わりに次が読める。
- プロセス ID・IPC・UTS・ユーザーの各名前空間は必ず切る。
- ユーザー名前空間を作れない環境では起動を拒否する。
- cgroup 名前空間は、カーネルが対応しない場合には切らない。
- 対応環境は setuid でない bwrap と root 以外の利用者。
Shown by: check —
```sh
rg -n 'cgroup, and user namespaces are always unshared' README.md docs/security.md  # 何も出ない
rg -n 'cgroup' README.md docs/security.md                                           # 条件付きの記述が出る
rg -n 'user namespace' README.md docs/security.md                                   # 拒否の条件が出る
git diff --check -- README.md docs/security.md
```
Left to the implementer: 文面と、4 点を 1 文にまとめるか分けるか。
Stop and hand back if: 実装が仕様 2 節と違う（例: cgroup 名前空間を常に切っている）のを見つけたら、文書を実装に寄せず止める。

### 手順 3 — getting-started の init と full 計画の説明を直す

Purpose: `init` が `profile/` まで 0700 で作ると読める文を `secrets/` だけに直し、`--print-plan=full` が加えるものの列挙に各マウント項目の出所と解決したコマンドのパスを足す。
Specification: `docs/spec/kakoi.md` 4.1 節「文法」（`init`）、13 節「出力と終了コードの契約」。
Prerequisites: なし。
May change: `docs/getting-started.md`。
Done when:
- 「creates `profile/` and `secrets/` (mode 0700)」と読める文が無く、0700 は `secrets/` にだけ掛かり、`profile/` と書き出すファイルはプロセスの umask に従うと読める（umask に触れるかは問わない。0700 が `secrets/` だけであることが読み取れればよい）。
- `--print-plan=full` の説明に、合成後のポリシー、最終の環境の全体、bwrap の引数列に加えて、各マウント項目の出所と、解決したコマンドのパスが含まれることが書かれている。
- 33〜37 行のユーザー名前空間の段落は変更されていない。
Shown by: check —
```sh
rg -n 'profile/` and `secrets/` \(mode 0700\)' docs/getting-started.md  # 何も出ない
rg -n '0700' docs/getting-started.md                                    # 出る。読み、secrets/ にだけ掛かることを確かめる
rg -n 'resolved command' docs/getting-started.md                        # 出る
git diff --check -- docs/getting-started.md
```
Left to the implementer: 文面。
Stop and hand back if: なし（計画全体の停止条件に従う）。

### 手順 4 — cli.md の入れ子・計画表示の記述を直す

Purpose: 入れ子の exec 失敗（126）の例から誤りの「カーネルが実行できない形式のファイル」を外し、入れ子の節と計画の説明に `--print-plan` 付き入れ子・組み込みの既定の表示・出所の注記を足す。
Specification: `docs/spec/kakoi.md` 4.2 節「コマンドの解決」、12.1 節「入れ子」、13 節「出力と終了コードの契約」。
Prerequisites: なし。
May change: `docs/cli.md`。
Done when:
- 「a file of a format the kernel cannot run」が無く、126 の例は「インタプリタが無いスクリプト」だけになっている（execvp は ENOEXEC で /bin/sh に落ちるため、実行できない形式のファイルは 126 の例にならない）。
- 入れ子の節に、`--print-plan` 付きの入れ子は計画を表示し、何も実行せず、警告も出さないこと、文字形の計画は `nested:` の行で始まることが書かれている（実装の典拠は `src/plan_text.rs:35-36`）。
- 計画の説明（`## The plan` の要約の列挙か本文）に、組み込みの既定を使ったときポリシーファイルの欄に文字列 `kakoi init` が出ることが書かれている（この文字列が契約）。
- 出所の注記の一覧に `(secrets/ of the configuration directory)` がある。
Shown by: check —
```sh
rg -n 'format the kernel cannot run' docs/cli.md             # 何も出ない
rg -n 'interpreter does not exist' docs/cli.md               # 出る
rg -n 'nested:' docs/cli.md                                  # 入れ子の節に出る
rg -c 'kakoi init' docs/cli.md                               # 2 以上（6 行目の構文に加えて計画の節に出る）
rg -n 'secrets/ of the configuration directory' docs/cli.md  # 出る
git diff --check -- docs/cli.md
```
Left to the implementer: 文面、入れ子の説明を既存の段落に足すか節を分けるか。`rg` の行番号は目安。
Stop and hand back if: `--print-plan` 付き入れ子の警告の有無や `nested:` の形が実装（`src/plan_text.rs`、`src/startup.rs`）と食い違うなら、文書に書かず止める。

### 手順 5 — shim.md の引数の説明を直す

Purpose: シムが包む対象の引数をそのまま `kakoi` に渡すという説明に、既定の `BYPASS_FLAG` を引数列の先頭に差し込むことを足す。
Specification: `docs/spec/kakoi.md` 16 節「公開ドキュメント」。
Prerequisites: なし。
May change: `docs/shim.md`。
Done when: 22〜24 行のあたりの段落が、対象の引数はそのままで、ツール節の既定の `BYPASS_FLAG` が引数列の先頭に置かれることを述べている。
Shown by: check —
```sh
sed -n '22,24p' docs/shim.md | rg -n 'arguments unchanged'  # 対象の段落が出る
sed -n '22,24p' docs/shim.md | rg -n 'flag'                  # 出る（変更前は出ない。差し込みに触れた証拠）
git diff --check -- docs/shim.md
```
Left to the implementer: 文面（`BYPASS_FLAG` という名前を使うか「the flag」と書くか。フラグが空のツール節でも引数は変わらないことが読み取れればよい）。
Stop and hand back if: なし（計画全体の停止条件に従う）。

### 手順 6 — policy.md のネットワークと生成項目を直す

Purpose: ネットワークの説明を仕様 7 節に合わせ、生成項目の走査の説明を、隠す対象と何もしない場合が読める形に狭める。
Specification: `docs/spec/kakoi.md` 6.2 節「実体への解決と存在しないパス」、6.3 節「生成される項目」、7 節「ネットワーク」。
Prerequisites: なし。
May change: `docs/policy.md`。
Done when:
- Network の節から「no network at all」という断定が消え、`none` はネットワーク名前空間を切ってループバックだけが残ると読める。
- 「combine `host` with an external proxy」という案内が消え、`none` にしたうえで外のプロキシの UNIX ソケットを `rw-file` で通し `env.set` で指す形が組めること、それが未検証でこの仕様が保証しないことが書かれている。
- 生成項目の `mounts.scan` の箇所が、隠すのは名前に一致するエントリのうちディレクトリとディレクトリに解決するリンクを除いたもの、読み込んだポリシーファイル自身は対象外、何の実体にもならないものは何もしない、と読める。
Shown by: check —
```sh
rg -n 'no network at all' docs/policy.md                      # 何も出ない
rg -n 'loopback' docs/policy.md                               # 出る
rg -n 'combine `host` with an external proxy' docs/policy.md  # 何も出ない
rg -n 'the files found by' docs/policy.md                     # 何も出ない（狭めた文に変わる）
rg -n 'mounts.scan' docs/policy.md                            # 狭めた説明が出る
git diff --check -- docs/policy.md
```
Left to the implementer: 文面、箇条書きの分け方。
Stop and hand back if: なし（計画全体の停止条件に従う）。

### 手順 7 — PROJECT.md のテスト・hook・境界の記述を直す

Purpose: テストが起動するコマンドのパスの説明をテストの実態に合わせ、pre-commit hook の前提（`run-if-present` が `PATH` に要る）を書き、実行時の境界の「ファイルを書かない」に `init` の例外を足す。
Specification: `docs/spec/kakoi.md` 14 節「実行時の境界」、15 節「検証の契約」（15.2 節「ビルド済みバイナリのテスト」）。hook の前提は仕様の外で、`lefthook.yml` に合わせる。
Prerequisites: `tests/launch.rs`、`tests/cli.rs`、`lefthook.yml` を読めること（読み取りだけ）。
May change: `PROJECT.md`。
Done when:
- テストのコマンドの説明が、次と読める。
  - 絶対パスで起動するのは、隔離の中の `/usr/bin/python3`・`/usr/bin/git`・`/bin/sh`・`/bin/true`、入れ子の中の `/usr/bin/env`・`/bin/sh`。
  - 一時ディレクトリに置いたコピーは絶対パスでは起動しない。`/bin/echo` と `/bin/cat` のコピーは相対名 `-x/tool` で、`/bin/sh` のコピーはホストの `PATH` 経由で名前 `sh` で起動する。
  - `python3` がシステムコールとソケットを観測する役であることは、意味が変わらない範囲で残してよい。
- pre-commit hook の段落に、hook のコマンドが `run-if-present`（mise で入れる `github:ba0918/run-if-present`。`PATH` に要る）で包まれていることが書かれ、`lefthook install` の案内は残っている。
- 「no files written」の制約が、ファイルを書くのは `init` だけ（書き先は仕様 14 節が定める範囲）と読め、`--print-plan` を含むコマンドを包む起動は書かないと読める。
Shown by: check —
```sh
rg -n 'by those absolute paths' PROJECT.md  # 何も出ない
rg -n 'x/tool' PROJECT.md                   # 出る
rg -n 'bin/true' PROJECT.md                 # 出る
rg -n 'run-if-present' PROJECT.md           # 出る
rg -n 'lefthook install' PROJECT.md         # 残っている
rg -n 'writ.*init|init.*writ' PROJECT.md    # 出る（制約の段落に init の例外。変更前は出ない）
git diff --check -- PROJECT.md
```
Left to the implementer: 文面、列挙の順。
Stop and hand back if: `tests/launch.rs` と `tests/cli.rs` を読み直した結果が、この手順の固定した意味（コピーは絶対パスで起動しない）と両立しない（例: コピーを絶対パスで起動するテストが増えている）なら、どちらにも寄せず止める。

### 手順 8 — CONTEXT.md の 3 語を直す

Purpose: プロファイルの置き場所を `profile/NAME.toml` にし、ホームディレクトリに `--print-plan` の無い入れ子の短絡を足し、ワークツリーの `.git` の見方（シンボリックリンクを辿らない）を足す。
Specification: `docs/spec/kakoi.md` 2 節「用語」（プロファイル、ワークツリー、ホームディレクトリ）、13 節「出力と終了コードの契約」。
Prerequisites: なし。`src/workspace_facts.rs:89-106` を読めること（読み取りだけ）。
May change: `CONTEXT.md`。
Done when:
- プロファイルの節の置き場所が `profile/NAME.toml` になっている。
- ホームディレクトリの節が、`--print-plan` の無い入れ子は仕様 13 節の段階 2 で短絡して段階 4 の `env` の診断に至らないと読める。
- ワークツリーの節が、`.git` はシンボリックリンクを辿らずに見て、ディレクトリまたは通常ファイルであるものだけを数えると読める。
Shown by: check —
```sh
rg -n 'profile/NAME.toml' CONTEXT.md  # 出る
rg -n '段階 2' CONTEXT.md             # 出る
rg -n '辿らず' CONTEXT.md             # ワークツリーの節に出る
git diff --check -- CONTEXT.md
```
Left to the implementer: 文面。3 箇所はそれぞれ独立して直してよい。
Stop and hand back if: なし（計画全体の停止条件に従う）。

## 変更しない範囲

- `docs/spec/kakoi.md`（承認済み。読むだけ。`Status` の行も変えない）。
- `src/`、`tests/`、`examples/`、`skills/`、`lefthook.yml`、`Cargo.toml`、`Cargo.lock`。
- 変更する範囲に挙げた「既に直っていて触らない箇所」。
- 用語の全面改名（英語文書の "layer" などを "stage" に変えるなど）。対象は各手順が名指しした主張だけで、用語の言い換えはしない。
- `docs/getting-started.md:33-37` のユーザー名前空間の段落。
- 0.2 で作らないもの（仕様 18 節）に関する新機能、新オプション、新キー。文書をその一覧に合わせる作業もしない（ずれとして報告されていない）。
