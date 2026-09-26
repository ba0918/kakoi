# コマンドライン

この章は、`kakoi` の呼び出し方、オプションの書き方、包んだコマンドの探し方、`kakoi init` の動きを定める。
コマンドラインを組み立てるとき、`usage` の診断の原因を探すとき、`init` の結果を確かめるときに読む。
計画の出力の中身は[計画の表示](04-plan.md)、診断と終了コードの全体は[入れ子、並列、出力、診断と終了コード](10-process.md)が扱う。

## 書式
<!-- @kotowari[REQ-250:e9baf30f, EX-490:9ecf645d] -->

`kakoi` が受け付ける呼び出しは次の 5 つの形だけである。

```
kakoi [OPTIONS] -- COMMAND [ARGS]...
kakoi [OPTIONS] --print-plan[=FORM] [-- COMMAND [ARGS]...]
kakoi init [NAME]
kakoi --version
kakoi --help
```

| 形 | 何をするか |
|---|---|
| `kakoi [OPTIONS] -- COMMAND [ARGS]...` | 隔離の中で `COMMAND` を実行する |
| `kakoi [OPTIONS] --print-plan[=FORM] ...` | 実行せずに計画を表示する（[計画の表示](04-plan.md)） |
| `kakoi init [NAME]` | 組み込みの既定をプロファイルとして書き出す（[init](#init-が書き出すもの)） |
| `kakoi --version` | 版を表示する |
| `kakoi --help` | 使い方を表示する |

実行の形では `--` と `COMMAND` の両方が必須で、`ARGS` は省略できる。
`--` より後ろの語は、オプションの形をしていても解釈せず、そのまま `COMMAND` の引数として渡す。

5 つの形のどれにも当てはまらない呼び出し（未知のオプション、`COMMAND` の欠落、形の混在）は、種類 `usage` の診断を出して終了コード 125 で終わる。

```console
$ kakoi -- claude --help        # --help は claude の引数になる
$ kakoi --rw ~/data              # COMMAND が無い
kakoi: usage: COMMAND is required unless --print-plan is given
```

この章の例に示す診断の説明の文言は現行の版の出力であり、変わりうる。
呼び出し元が頼れるのは、診断の種類（`usage` や `path`）と終了コードである（[入れ子、並列、出力、診断と終了コード](10-process.md)）。

## オプション
<!-- @kotowari[REQ-251:d55808ce, REQ-254:a089853e] -->

| オプション | 値 | 既定 | 意味 |
|---|---|---|---|
| `--profile NAME` | プロファイル名 | `default` | グローバルスコープとして読むプロファイル（設定ディレクトリの `profile/NAME.toml`）を選ぶ |
| `--policy-file PATH` | パス | 無し | プロセススコープとして今回だけ重ねるポリシーファイル。指定先が無ければ種類 `policy` の診断 |
| `--workspace PATH` | パス | カレントディレクトリ | ワークスペース（変数 `${workspace}` の値） |
| `--rw PATH` | パス | 無し | コマンドラインの段へ `rw` の項目を足す。繰り返せる |
| `--hide PATH` | パス | 無し | コマンドラインの段へ `hide` の項目を足す。繰り返せる |
| `--print-plan[=FORM]` | `summary`、`full`、`json` | `summary` | 実行せずに計画を表示する（[計画の表示の指定](#計画の表示の指定)） |
| `--version` | 無し | 無し | 版を標準出力へ出して 0 で終わる |
| `--help` | 無し | 無し | 使い方を標準出力へ出して 0 で終わる |

`--profile` と `init` の `NAME` は、有効な UTF-8 で空でない 1 つのパス要素でなければならない。
次のどれかに当たる名前は種類 `usage` の診断で終わる。
名前が `profile/` の外のファイルを指せないようにするためである。

- `/` を含む
- 制御文字（0x00〜0x1F、0x7F）を含む
- 単独の `.` または `..`

`NAME` を省略したときと `default` を明示したときは同じ意味になる。
プロファイル、段、グローバルスコープ、プロセススコープの意味は[用語](02-terms.md)を、ポリシーの合成は[ポリシー](05-policy.md)を参照する。

```console
$ kakoi --profile ../x -- sh
kakoi: usage: the profile name `../x` is not a single path component
```

## オプションの書き方
<!-- @kotowari[REQ-252:2e7b9c8b, EX-492:99679867] -->

値を取るオプションは `--opt VALUE`（分離形）と `--opt=VALUE`（`=` 形）のどちらでも書け、意味は同じである。
次の規則に反すると、種類 `usage` の診断で終了コード 125 になる。

| 規則 | 違反の例 |
|---|---|
| 繰り返せるのは `--rw` と `--hide` だけ。他のオプションは 1 回まで | `--profile a --profile b` |
| 値は空にできない（どちらの形でも） | `--workspace=`、`--rw ""` |
| 分離形で次の語が `-` で始まるときは、値の欠落として扱う | `--rw --hide x` |

`-` で始まる値を渡したいときは `=` 形で書く（`--rw=-dir`）。
`--rw` と `--hide` を繰り返したときは、書いた順にすべてを保持する。

```console
$ kakoi --rw /a --rw /b -- sh    # /a と /b の 2 件を順に足す
$ kakoi --rw -- sh
kakoi: usage: --rw needs a value that is not empty and does not start with `-`
```

## コマンドラインで渡すパス
<!-- @kotowari[REQ-254:a089853e, EX-494:905a8cbc] -->

`--policy-file`、`--workspace`、`--rw`、`--hide` に渡す相対パスは、カレントディレクトリからの相対として解釈する。
コマンドラインの値では `~` も変数（`${worktree}` の形）も展開しない。
`~` や変数の展開はポリシーファイルに書く値だけの規則である（[ポリシー](05-policy.md)）。
シェルが展開しない位置に書いた `~` は、`~` という名前のディレクトリとして扱われる。

```console
$ cd /cwd
$ kakoi --hide '~/x' -- sh       # /cwd/~/x を隠す（ホームディレクトリの x ではない）
```

`--workspace` を省略すればカレントディレクトリがワークスペースになる。
`--policy-file` は省略でき、指定したファイルが無ければ種類 `policy` の診断で終わる。

## 計画の表示の指定
<!-- @kotowari[REQ-253:d81119e3, EX-493:21c50141] -->

`--print-plan` を付けると、コマンドを実行せずに計画を表示する。
形式 `FORM` は `=` 形でだけ指定でき、値は次の 3 つに限られる。

| FORM | 表示 |
|---|---|
| `summary`（省略時） | 人が読む要約 |
| `full` | 合成後のポリシー、出所、最終の環境、bwrap の引数列を含む全量 |
| `json` | ツール向けの 1 行の JSON |

各形式の中身は[計画の表示](04-plan.md)が定める。

`--print-plan` では `-- COMMAND` を省略できる。
省略すると、計画のコマンドの欄は無しになる。
ただし `--` を書いて `COMMAND` を書かなければ種類 `usage` の診断になる。

| 書き方 | 結果 |
|---|---|
| `--print-plan` | 要約を表示 |
| `--print-plan=full` | 全量を表示 |
| `--print-plan full` | `usage`、125（分離形は受け付けない） |
| `--print-plan=` や `--print-plan=yaml` | `usage`、125 |
| `--print-plan --` | `usage`、125（`--` の後に `COMMAND` が無い） |

## --help と --version
<!-- @kotowari[REQ-255:b4b437d2, EX-495:ff90588d] -->

`--help` は使い方を、`--version` は版を標準出力へ出し、終了コード 0 で終わる。
どちらもカレントディレクトリを取得しないので、削除済みのディレクトリの中から実行しても動く。

どちらも単独でだけ使える。
他のオプション、`--`、`COMMAND` のどれと併用しても種類 `usage` の診断で終了コード 125 になる。

```console
$ kakoi --version
kakoi 0.3.0                      # 版の数字は入れた版による
$ kakoi --help -- sh
kakoi: usage: --help and --version cannot be combined with any other argument
```

## 実行するコマンドの探し方
<!-- @kotowari[REQ-260:5288d70d, REQ-261:bd6588a2, EX-500:d93cef11, EX-501:fb8d778f, REQ-446:a7751adc] -->

`kakoi` は起動の前に `COMMAND` をファイルのパスへ解決する。

| `COMMAND` | 探し方 |
|---|---|
| `/` を含む（`./tool`、`/opt/tool`） | そのパスが存在し、実行可能であることを確かめる |
| `/` を含まない（`sh`） | 隔離の中へ渡す `PATH` の要素を順に探す。その `PATH` が無ければ探さない |

どちらの場合も見つからなければ、種類 `command not found` の診断を出して終了コード 127 で終わる。
診断の説明は `COMMAND` に与えた名前である。
探すのに使う `PATH` は、ポリシーで組み立てた隔離の中の値である（[環境変数](08-environment.md)）。
`/` を含まない名前を見張り役と同じ探し方で探した結果が、見張り役を置いたプログラムの本物なら、そのコマンドも見張り役を通して起動し、計画のコマンドの欄のパスは見張り役のパスになる（[コマンドのガードレール](13-command-guard.md#見張り役の置き方)）。

```console
$ kakoi -- /opt/tool             # /opt/tool が存在しない
kakoi: command not found: /opt/tool
```

次の 2 つの場合は探し方が変わる。

- `--print-plan` で `COMMAND` を省略したときは解決を行わず、計画のコマンドの欄は無しになる。
- 入れ子（環境変数 `KAKOI=1` の状態で `kakoi` が起動されること）では、`--print-plan` の有無にかかわらず、`kakoi` が受け取ったホストの `PATH` で探す。ホストの `PATH` が無ければ探さない。入れ子で計画を表示すると、計画に載る `PATH` は隔離の中の値のまま、コマンドの欄はホストの `PATH` で解決した結果になる。

入れ子の扱い全体は[入れ子、並列、出力、診断と終了コード](10-process.md)を参照する。

## 見つかった後に実行できない場合
<!-- @kotowari[REQ-262:837c8d32, EX-502:6820cb31] -->

見つかったコマンドの exec が失敗したとき（たとえばインタプリタが無いスクリプト）の見え方は、通常の起動と入れ子で異なる。

| 起動 | 見え方 |
|---|---|
| 通常の起動 | bwrap の失敗として、bwrap の出力と終了コードがそのまま返る |
| 入れ子 | 種類 `command not executable` の診断で終了コード 126。説明はパスとエラー |

通常の起動では exec するのは bwrap であり、入れ子では `kakoi` 自身がコマンドを exec するためである。
2 つの経路で失敗の見え方が違うことは許容している。

## argv[0] に渡す名前
<!-- @kotowari[REQ-263:50ac75c0, EX-503:593ea85f] -->

起動したプロセスの argv[0] は、通常の起動でも入れ子でも、`COMMAND` に与えた文字列そのものである。
解決したパスは exec するファイルを指定するためだけに使い、bwrap には `--argv0` で与えた名前を渡す。
シェルが `PATH` で見つけたコマンドに打った名前を argv[0] として渡すのと同じ動きで、argv[0] で振る舞いを変えるコマンド（多重呼び出しのバイナリ）が打ったとおりに動く。

```console
$ kakoi -- sh -c 'echo $0'       # /usr/bin/sh に解決しても argv[0] は sh
sh
```

## 探索と隔離の間に残る隙間
<!-- @kotowari[REQ-264:1e6384ea, EX-504:93b0838a] -->

コマンドの探索はホストのファイルシステムで行う。
そのため、`hide` で隠したディレクトリの中にあるコマンドも「見つかった」と判定され、その後に bwrap の exec の失敗として終わる。
この隙間は許容しており、README に記す。

引き継いだ `PATH` の要素が `rw` の項目の中にあっても検査しない。
解決したコマンドは隔離の中で動くので、隔離の中からその実体を差し替えられても境界の外には届かないためである。

## init が書き出すもの
<!-- @kotowari[REQ-256:a3da3d2e, EX-496:05db4ead] -->

`kakoi init [NAME]` は、組み込みの既定を設定ディレクトリの `profile/NAME.toml` に書き出す。
`NAME` を省略すれば `default` になり、`NAME` の制約は `--profile` と同じである（[オプション](#オプション)）。
設定ディレクトリは `$XDG_CONFIG_HOME/kakoi/`、それが未設定（空や絶対パスでない値を含む）なら `~/.config/kakoi/` である（[用語](02-terms.md)）。

書き出す内容は、同梱の `examples/profile/default.toml` と同じバイト列である。
次のディレクトリが無ければ作る。

- 設定ディレクトリの祖先（`~/.config` や `XDG_CONFIG_HOME` の指す先）
- 設定ディレクトリ自身
- `profile/`
- `secrets/`

無い設定ディレクトリは新しい機械の普通の状態であり、作るのは利用者の環境変数が指す場所だけである。

```console
$ kakoi init                     # 設定ディレクトリが無い状態から
/home/alice/.config/kakoi/profile/default.toml
$ kakoi init strict
/home/alice/.config/kakoi/profile/strict.toml
```

## init のリンクの辿り方とモード
<!-- @kotowari[REQ-257:1f5cd0b7, EX-497:e965ea2b] -->

書き出す先の名前より上のパス成分（設定ディレクトリ、`profile/`、`secrets/`）は、シンボリックリンクを通常どおり辿る。
利用者が dotfiles へのリンクで設定ディレクトリを置く使い方に合わせている。
辿った先がリンク切れのとき、またはディレクトリでない（通常ファイルがある）ときは、種類 `path` の診断で終わる。

作成するものと既存のもののモードは次のとおり。

| 対象 | モード |
|---|---|
| `secrets/` | 新しく作るときも既にあるときも 0700 にする |
| `profile/` | プロセスの umask に従う |
| 書き出すファイル | プロセスの umask に従う |

`secrets/` を 0700 に揃えるのは、利用者が手で作った広いモードの置き場を残さないためである。
プロファイルは秘密ではなく、利用者が読み、dotfiles に置くものなので umask に任せる。

## init の出力と上書きの拒否
<!-- @kotowari[REQ-258:15b6f0fa, EX-498:04d2152e] -->

成功すると、書き出したファイルのパスを 1 行だけ標準出力に出し、終了コード 0 で終わる。
出すパスは設定ディレクトリの規則で組み立てたままのパスで、シンボリックリンクを実体に解決しない。
`$EDITOR "$(kakoi init)"` のように、出力をそのまま次のコマンドに渡せる。

書き出す先の名前に何かが既にあれば、何も書かずに種類 `path` の診断で終了コード 125 になる。
既にあるものの種類は問わない（通常ファイル、ディレクトリ、リンク切れを含むシンボリックリンク）。
書き出す先の名前自身がシンボリックリンクでも辿らないので、リンク先を作ることもない。

```console
$ kakoi init
/home/alice/.config/kakoi/profile/default.toml
$ kakoi init                     # 2 回目
kakoi: path: /home/alice/.config/kakoi/profile/default.toml already exists
```

上書きする形（`--force`）は持たない。
隔離の境界を記述したファイルを製品が上書きする手段を置かないためで、書き直すときは利用者が消してから `init` する。
書き込みに失敗したときの診断の説明は、対象のパスを含む。

既に書き出す先があるときに「何も書かず」が `secrets/` のモード変更まで含むかは決まっていない（[未決事項](../appendix/open-issues.md)）。

## init が行う検査と行わない検査
<!-- @kotowari[REQ-259:12198cab, EX-499:6fd29e1c] -->

`init` は次の順で検査し、最初に当たった診断で終わる。

1. 文法（`usage`）。`NAME` 以外の引数（オプション、`--`、`COMMAND`）を併用すると、ここで終わる。
2. ホームディレクトリ（`env`）。`XDG_CONFIG_HOME` が設定されていても行う。
3. 書き込み（`path`）

`init` は、ポリシーの読み込み、ワークスペースの導出、マウントの解決、`bwrap` の所在の確認、入れ子の検出を行わない。
`bwrap` の無い機械でも、隔離の中でも、書ける場所であれば設定を置ける。
たとえば `KAKOI=1` で `bwrap` の無い環境でも、書き込みの条件を満たせば入れ子の警告を出さずに成功する。

```console
$ kakoi init -- sh
kakoi: usage: init takes nothing but a profile name
```
