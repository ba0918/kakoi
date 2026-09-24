# ポリシーファイルの形式と合成

この章は、**ポリシーファイル**（kakoi に渡すポリシーを書いた TOML ファイル。[用語](02-terms.md)）に書けるキー、パスの書き方と変数、複数の段を重ねて 1 つのポリシーにする規則、同梱プロファイルの中身を定める。
ポリシーファイルを書くとき、書いたものがなぜ効かないか、なぜ読み込みに失敗するかを調べるときに読む。
パスの書き方が正しくても、隔離の中から次回のポリシーを差し替えられる置き場は拒否される。
その条件は [ポリシーを保護する配置](06-policy-placement.md) が定める。

## キーの一覧
<!-- @kotowari[REQ-151:21ed6c60] -->

ポリシーファイルに書けるキーは次の表のとおりである。
表にない固定キーを書くと、ポリシーの読み込みに失敗する（種類 `policy` の診断、終了コード 125）。
`env.set`、`secrets`、`git.instead-of` の 3 つのテーブルだけは、キー名そのものを利用者が決める。

「既定値」の列の「空」は、書かなければ項目が 1 つも無いことを表す。
セクションはどれも省略できる。

| セクション | キー | 型 | 既定値 | 意味 | 詳細 |
|---|---|---|---|---|---|
| `mounts` | `rw` | パスの配列 | 空 | 読み書きできるようにする場所 | [マウント](07-mounts.md) |
| `mounts` | `rw-file` | パスの配列 | 空 | 読み書きできるようにする単独のファイル | [マウント](07-mounts.md) |
| `mounts` | `rw-copy` | パスの配列 | 空 | 複製を置き、書き込みをホストへ戻さない場所 | [マウント](07-mounts.md) |
| `mounts` | `ro` | パスの配列 | 空 | 読み取り専用にする場所 | [マウント](07-mounts.md) |
| `mounts` | `hide` | パスの配列 | 空 | 中身を見えなくする場所 | [マウント](07-mounts.md) |
| `mounts` | `scan` | テーブルの配列 | 空 | 名前で探して `hide` を生成する走査 | [マウント](07-mounts.md) |
| `mounts.scan` の項目 | `root` | パス | 必須 | 走査の起点 | [マウント](07-mounts.md) |
| `mounts.scan` の項目 | `names` | 文字列の配列 | 必須（空は不可） | 隠す名前のパターン | [ワイルドカード](#ワイルドカード) |
| `mounts.scan` の項目 | `exclude` | 文字列の配列 | 空 | `names` に一致しても隠さない名前のパターン | [マウント](07-mounts.md) |
| `mounts.scan` の項目 | `prune` | 文字列の配列 | 空 | 中へ降りないディレクトリ名のパターン | [マウント](07-mounts.md) |
| `mounts` | `hide-mounts` | テーブルの配列 | 空 | ファイルシステムの種類でマウントを隠す指定 | [マウント](07-mounts.md) |
| `mounts.hide-mounts` の項目 | `under` | パス | 必須 | 探す範囲の起点 | [マウント](07-mounts.md) |
| `mounts.hide-mounts` の項目 | `fstype` | 文字列の配列 | 必須（空は不可） | 隠すファイルシステムの種類 | [マウント](07-mounts.md) |
| `env` | `mode` | 文字列 `"inherit"` `"clear"` | 表の下を参照 | ホストの環境変数を引き継ぐか、空から始めるか | [環境変数](08-environment.md) |
| `env` | `pass` | 文字列の配列 | 空 | `mode = "clear"` のときに残す変数名 | [環境変数](08-environment.md) |
| `env` | `set` | テーブル（変数名 = 文字列） | 空 | 設定する変数と値 | [環境変数](08-environment.md) |
| `env` | `unset` | 文字列の配列 | 空 | 消す変数名のパターン | [ワイルドカード](#ワイルドカード) |
| `env` | `path-prepend` | パスの配列 | 空 | `PATH` の先頭に足すディレクトリ | [環境変数](08-environment.md) |
| `secrets` | 変数名 | パス | 空 | 変数に入れる秘密を読むファイル | [環境変数](08-environment.md) |
| `git` | `instead-of` | テーブル（URL = URL） | 空 | Git の URL の書き換え | [環境変数](08-environment.md) |
| `network` | `mode` | 文字列 `"host"` `"none"` `"filtered"` | 表の下を参照 | ネットワークモード | [モード](network/01-modes.md) |
| `network` | `allow` | テーブルの配列 | 空 | filtered で通す通信（`destination`、`protocol`、`ports`） | [通信許可](network/02-allow.md) |
| `network` | `publish` | テーブルの配列 | 空 | ホストへ公開するポート（`mode`、`protocol`、`port`、`host-port`、`target-family`、`host-family`） | [公開](network/06-publish.md) |
| `network` | `dns-upstream` | テーブルの配列 | 空（ホストの DNS を使う） | 明示する上流 DNS | [上流 DNS](network/05-dns-upstream.md) |
| `network` | `dns-server-timeout-seconds` | 整数（1〜300） | 2 | 上流 DNS の 1 候補を待つ秒数 | [上流 DNS](network/05-dns-upstream.md) |
| `network` | `dns-resolution-timeout-seconds` | 整数（1〜3600） | 10 | 名前解決全体の上限の秒数 | [上流 DNS](network/05-dns-upstream.md) |
| `network` | `dns-max-upstream-queries` | 整数（1〜4096） | 64 | 名前解決 1 件あたりの上流への問い合わせ回数 | [上流 DNS](network/05-dns-upstream.md) |
| `network` | `dns-max-cname-hops` | 整数（1〜128） | 16 | CNAME を辿る段数 | [上流 DNS](network/05-dns-upstream.md) |
| `network` | `dns-max-concurrent-resolutions` | 整数（1〜4096） | 256 | 同時に処理する名前解決の数 | [上流 DNS](network/05-dns-upstream.md) |
| `network` | `dns-max-waiters-per-resolution` | 整数（1〜1024） | 64 | 共有する解決 1 件あたりの応答待ちの数 | [上流 DNS](network/05-dns-upstream.md) |
| `network` | `dns-zero-ttl-grace-milliseconds` | 整数（100〜10000） | 1000 | TTL が 0 の応答で得た IP へ新しく接続できる猶予 | [DNS](network/04-dns.md) |
| `network` | `udp-idle-timeout-seconds` | 整数（1〜86400） | 120 | UDP の無通信期限 | [DNS](network/04-dns.md) |
| `process` | `shutdown-grace-seconds` | 整数（1〜300） | 5 | filtered で終了を待つ猶予 | [プロセス監督](network/07-lifecycle.md) |

`network.allow`、`network.publish`、`network.dns-upstream` の各項目の中のキーと、`network` と `process` のキーを書いたときに `network.mode` の明示が要る条件は、それぞれの詳細の章が定める。

`env.mode` と `network.mode` の「表の下を参照」は、どの段も書かなかったときの値が合成の後に決まることを表す（[モードを省略したときの値](#モードを省略したときの値)）。
同梱プロファイルは両方を明示している（`env.mode = "inherit"`、`network.mode = "host"`）。

## ファイルの形式
<!-- @kotowari[REQ-151:21ed6c60, EX-350:b2bc402d, EX-351:620df7f9] -->

ポリシーファイルは TOML で書く。
次の例は形を示すためのもので、値は例である。

```toml
[mounts]
rw      = ["${workspace}", "${worktree}", "${git_common_dir}", "/tmp/kakoi", "~/.cache"]
rw-file = ["~/.claude.json"]
rw-copy = ["~/.gitconfig"]
ro      = ["~/.codex/AGENTS.md"]
hide    = ["/tmp", "/run/user", "~/.ssh", "~/.aws", "/run/WSL"]

[[mounts.scan]]
root    = "${worktree}"
names   = [".env", ".env.*"]
exclude = ["*.example", "*.sample", "*.dist", "*.template"]
prune   = [".git", "node_modules"]

[[mounts.hide-mounts]]
under  = "/mnt"
fstype = ["9p", "drvfs"]

[network]
mode = "host"

[env]
mode         = "inherit"
pass         = []
set          = { }
unset        = ["SSH_AUTH_SOCK", "DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY", "*_TOKEN", "*_API_KEY"]
path-prepend = []

[secrets]
GH_TOKEN = "${config_dir}/secrets/gh-token"

[git.instead-of]
"git@github.com:" = "https://github.com/"
```

空のファイルも有効なポリシーファイルで、そのまま受け入れられる。

次の場合はポリシーの読み込みに失敗し、種類 `policy` の診断で終わる（終了コード 125）。

- TOML として解析できない。
- 表にない固定キーを書いた（例: `mounts` のつもりで `mount = []` と書いた）。
- `mounts.scan` の項目に `root` か `names` が無い、または空である。
- `mounts.hide-mounts` の項目に `under` か `fstype` が無い、または空である。

## パスの書き方
<!-- @kotowari[REQ-152:d01ba8cf, EX-352:b99e9ee7] -->

パスを取る値は、次の 4 つの形のどれかで書く。
パスを取る値とは、`mounts` の 5 つの指令（`rw`、`rw-file`、`rw-copy`、`ro`、`hide`）の項目、`scan.root`、`hide-mounts.under`、`secrets` の値、`env.path-prepend` の項目である。

| 形 | 例 | 展開後 |
|---|---|---|
| 絶対パス | `/tmp/kakoi` | そのまま |
| `~` 単独 | `~` | ホームディレクトリ |
| `~/` で始まる | `~/.cache` | ホームディレクトリの下 |
| 変数で始まる | `${worktree}/build` | 変数の値の下（[パス変数](#パス変数)） |

相対パスと `~ユーザー名` の形は、ポリシーの読み込みに失敗する（種類 `policy`）。

`~` は**ホームディレクトリ**（ホストの環境変数 `HOME` の実体のパス。[用語](02-terms.md)）に展開する。
`HOME` の字面は使わない。
たとえば `HOME` が `/home/link` で、その実体が `/home/u` なら、`~/cache` は `/home/u/cache` になる。
`HOME` そのものが無い、空、相対パス、ディレクトリでない場合は、種類 `env` の診断になる（[用語](02-terms.md) の「ホームディレクトリ」）。

## パス変数
<!-- @kotowari[TBL-151:34959d44, REQ-152:d01ba8cf, REQ-153:d5cc7181, EX-354:c38885a0, EX-355:8d250a7f] -->

パスの先頭には次の 4 つの変数を書ける。
値はすべて実体のパス（シンボリックリンクを解決した後のパス）で、変数を導出するときに解決する。
kakoi はこれらの値を git コマンドを実行せず、ファイルシステムから導出する。

| 変数 | 値 |
|---|---|
| `${workspace}` | ワークスペースの実体のパス |
| `${worktree}` | ワークツリーの実体のパス |
| `${git_common_dir}` | 検証した共有 `.git` の実体のパス（[Git の共有ディレクトリ](#git-の共有ディレクトリ)）。ワークツリーに `.git` が無ければ値を持たない |
| `${config_dir}` | 設定ディレクトリの実体のパス。設定ディレクトリが存在しなければ値を持たない |

`${config_dir}` が値を持たないのは、設定ディレクトリが無く、組み込みの既定で起動したときに起きる（[プロファイルの選び方](#プロファイルの選び方)）。

### 値を持たない変数を含む項目

値を持たない変数を含む項目は、存在しないパスと同じ扱いで飛ばされる。

| 項目 | 扱い |
|---|---|
| マウント項目、走査の `root`、`hide-mounts.under`、`env.path-prepend` の項目 | 飛ばし、そのことを理由付きで計画に表示する |
| `secrets` の値 | 存在しない秘密ファイルとして警告になる（[環境変数](08-environment.md)） |

たとえば Git の管理下にないディレクトリで起動すると、`${git_common_dir}` で始まる `rw` の項目は飛ばされ、[計画](04-plan.md)に理由付きで残る。
計画には、値を持つ変数がすべて実体のパスで表示され、値を持たない変数はそう表示される。

### 展開しない値

変数展開はパスを取る値にだけ効く。
`env.set` の値には効かず、`${worktree}` と書けばその文字列のまま変数に入る。

### 変数に関する診断

| 条件 | 診断 |
|---|---|
| 4 つ以外の変数名を書いた | 種類 `policy`（読み込み失敗） |
| ワークスペースが存在しない、または実体がディレクトリでない（例: `--workspace` に通常ファイルを渡した） | 種類 `path`、終了コード 125 |
| 設定ディレクトリが存在するのに実体のパスを得られない（途中が辿れない、実体がディレクトリでない） | 種類 `path` |

## Git の共有ディレクトリ
<!-- @kotowari[REQ-152:d01ba8cf, EX-353:626797d8] -->

`${git_common_dir}` は、リンクされたワークツリー（`git worktree add` で作ったもの）やサブモジュールでも、共有される `.git` を書き込めるようにするための変数である。
値は、git 自身が作る相互のリンクを確かめられた場合だけ持つ。

まずワークツリーの `.git` をシンボリックリンクを辿らずに見る。

| `.git` の状態 | 値 |
|---|---|
| ディレクトリ | そのディレクトリ |
| 通常ファイル | 中の `gitdir:` が指すディレクトリを G として、下の 2 つの形のどちらかが成り立てばその値。どちらでもなければ種類 `path` の診断 |

リンクされたワークツリーの形では、次のとおり決める。
G の `commondir` ファイルの内容を G からの相対で解決したものを C とする。
次の 3 つがすべて成り立てば、値は C である。

1. G が `C/worktrees/` の直下にある。
2. `G/gitdir` ファイルの内容が、ワークツリーの `.git` ファイルを指している。
3. C の直下に通常ファイル `HEAD` がある（`HEAD` がシンボリックリンクなら辿らず、内容は読まない）。

サブモジュールの形では、G に `commondir` が無く、G の `config` ファイルの `core.worktree` が G からの相対でワークツリーを指していれば、値は G である。

これらの比較はすべて実体のパスで行う。
`.git` と、G の下で読むファイル（`commondir`、`gitdir`、`config`）は、開くファイル自身がシンボリックリンクなら辿らない（途中のディレクトリのリンクは辿る）。
通常ファイルでないものは、相互リンクが成り立たない場合と同じく種類 `path` の診断で終わる。
git 自身はこれらをシンボリックリンクでは作らず、G は隔離の中から書ける場所にありうるためである。
読む長さには上限がある（[実行時の境界](../maintainer/runtime.md)）。

`HEAD` の条件は偽造を止めるためにある。
C は G の置き場所だけで決まるので、隔離の中から書ける場所を G にして `commondir` を上の階層へ向ければ、`.git` ファイルとワークツリーの中身だけで C を好きな場所にできてしまう（ワークツリーやその親の名前が `worktrees` のとき、または `rw` の項目やその親の名前が `worktrees` のとき）。
`C/HEAD` を隔離の中から作れるなら、C は既に書き込める場所なので、新しく書き込めるようになるものは無い。
`HEAD` が既にある書き込めない C の `worktrees/` の直下に書き込める項目がある配置は、git も同梱プロファイルも作らない。

## プロファイルの選び方
<!-- @kotowari[REQ-154:a401a486, EX-356:b625a1c7, EX-357:bfdff5e5] -->

ポリシーは、最大 3 つの書かれた段を下から重ねて作る。

| 段（下から） | 与え方 |
|---|---|
| グローバルスコープ | `--profile NAME` で選ぶプロファイル `<設定ディレクトリ>/profile/NAME.toml`。省略時は `default` |
| プロセススコープ | `--policy-file` で指定したポリシーファイル |
| コマンドライン | `--rw`、`--hide` のようなマウントの引数（[コマンドライン](03-cli.md)） |

この上に、kakoi が生成する段（走査や `hide-mounts` が足す `hide`）が乗る（[マウント](07-mounts.md)）。

グローバルスコープのファイルが無いときの扱いは、`--profile` の与え方で分かれる。

| `--profile` | `profile/<名前>.toml` が無いとき |
|---|---|
| 省略、または `default` | 組み込みの既定がグローバルスコープになる |
| `default` 以外の名前 | ポリシーの読み込みに失敗する（種類 `policy`） |

`default` 以外を指定したときは `default.toml` を読まない。
名前を指定した利用者はそのファイルがあると思っているので、組み込みの既定へ落ちることはない。

**組み込みの既定**は、kakoi のバイナリに埋め込まれた同梱プロファイルと同じ内容である（[同梱プロファイル](#同梱プロファイル)）。
段を 1 つ増やすのではなく、ファイルの代わりにグローバルスコープの中身になるだけで、その上に `--policy-file` とコマンドラインが同じ規則で重なる。
組み込みの既定で起動しても、起動のたびに警告や通知は出さない。
`--print-plan` の「使ったポリシーファイル」の欄が、組み込みの既定を使ったことを文字列 `kakoi init` で示す（[計画の表示](04-plan.md)）。
`profile/default.toml` を置くと、それだけが読まれ、組み込みの既定は使われない。

### 「無い」の判定

「無い」とは、組み立てたパス `<設定ディレクトリ>/profile/default.toml` の各成分を根から順に見て（設定ディレクトリの祖先も含む）、最初に失敗した成分が名前として存在しない（リンクを辿らずに見て無い）ことである。
ディレクトリがまだ無いのは新しい機械の普通の状態なので、「無い」に数える。

次の場合は「ある」に数え、辿れないのでポリシーの読み込みに失敗する（種類 `policy`）。

- 最初に失敗した成分がリンク切れのシンボリックリンクである（`default.toml` 自身でも、設定ディレクトリでも）。
- 最初に失敗した成分が、ディレクトリがあるべき場所にある通常ファイルである。
- 全成分を辿れるのに `default.toml` を読めない。

厳しくしたプロファイルが壊れたときに、黙って広い既定で動かないためである。

`XDG_CONFIG_HOME` がまだ clone していない dotfiles の中を指している間は、組み込みの既定で動く。

### 例

設定ディレクトリが無い状態で起動する。

```console
$ kakoi --print-plan                   # 終了コード 0。計画は組み込みの既定を使ったことを示す
$ kakoi --profile strict --print-plan
kakoi: policy: ...                     # 終了コード 125
```

同じ状態で `--policy-file` を与えると、組み込みの既定の上にそれが重なる。
その `--policy-file` に `${config_dir}` で始まる `ro` の項目があれば、値を持たない変数を含む項目として飛ばされ、理由付きで計画に出る。

## 段の合成規則
<!-- @kotowari[TBL-152:121b5da2, REQ-154:a401a486, REQ-155:c0592024] -->

段を重ねる規則は、値の種類で決まる。

| 種類 | 対象 | 規則 |
|---|---|---|
| リスト | `mounts.rw` `rw-file` `rw-copy` `ro` `hide` `scan` `hide-mounts`、`env.pass` `unset` `path-prepend` | 連結する。上の段が下の段に足す |
| スカラー | `network.mode`、`env.mode` | 上の段が上書きする |
| テーブル | `env.set`、`secrets`、`git.instead-of` | キー単位でまとめる。同じキーは上の段が勝つ |

`env.path-prepend` の連結では、上の段の項目が先頭側に来る。
`PATH` に足したとき、上の段の項目ほど前に置かれる。

削除の演算子は無い。
下の段の項目を上の段で消すことも、テーブルのキーを「無し」に戻すこともできない。
それが必要なら別のプロファイルを作る。

`network` と `process` の追加キーも連結や上書きで重なる。
その規則は各キーの詳細の章が定める。

### 例

```toml
# プロファイル
[mounts]
rw = ["~/.cache"]
[env]
path-prepend = ["~/bin"]
set = { EDITOR = "vi", LANG = "C.UTF-8" }
```

```toml
# --policy-file
[mounts]
rw = ["~/.npm"]
[env]
path-prepend = ["~/project-tools/bin"]
set = { EDITOR = "nano" }
```

合成後は `mounts.rw` が `~/.cache` と `~/.npm` の 2 つ、`env.set` が `EDITOR = "nano"` と `LANG = "C.UTF-8"`、`PATH` の先頭は `~/project-tools/bin`、`~/bin` の順になる（パスは展開後の実体）。

## モードを省略したときの値
<!-- @kotowari[REQ-399:a12cc88f, EX-750:79097c75, EX-751:03954215, EX-752:557080d6] -->

`network.mode` と `env.mode` は、段を合成した後に、どの段も書いていなければ次の値になる。
ある段が書いていれば、合成の規則（上の段が上書きする）で決まった値を使う。

| キー | どの段も書いていないときの値 |
|---|---|
| `network.mode` | `"host"` |
| `env.mode` | `"inherit"` |

`network.mode` には例外が 1 つある。
`filtered` とともに加えたネットワークのキー（`network.allow`、`network.publish`、補助のキー）をどれかの段が書いたときは、既定値の `"host"` を補わない。
その場合にどの段にも `network.mode` が無ければ、種類 `policy` の診断で終わる（終了コード 125）。
キーの範囲とモードを明示する条件は[モード](network/01-modes.md)が定める。

```toml
# どの段も network.mode を書いていない
[network]
publish = []
```

この段だけでは `host` にならず、種類 `policy` の診断で終わる。

`env.mode` の既定値の `"inherit"` も、[合成した後の検査](#合成した後の検査)の対象になる。
どの段も `env.mode` を書かずに `env.pass = ["HOME"]` を書くと、合成後の `env.mode` が `inherit` なので、種類 `policy` の診断で終わる。

## ワイルドカード
<!-- @kotowari[REQ-155:c0592024, EX-358:bd9f2df1] -->

`env.unset` の項目と、走査の `names`、`exclude`、`prune` にはシェルのワイルドカードを使える。
意味はどこでも同じである。

| 文字 | 一致するもの |
|---|---|
| `*` | 0 文字以上の任意の並び。名前の先頭の `.` にも一致する |
| `?` | 任意の 1 バイト |
| それ以外 | その文字そのもの |

文字クラス（`[...]`）とエスケープは無い。
シェルと違い、`*` はファイル名 `.env` に一致する。

## マウント項目の同一性と競合
<!-- @kotowari[REQ-156:ada7ce62, EX-360:81655952, EX-361:5242712e] -->

`mounts` の 5 つの指令の項目が「同じパス」かどうかは、チルダと変数を展開し、シンボリックリンクを解決した実体のパスで判定する。
実体が存在しないパスは、チルダと変数を展開した後の文字列で判定する（書いたままの字面ではない）。

| 状況 | 結果 |
|---|---|
| 同じ段の中で、同じパスに同じ指令が 2 回 | 1 つにまとめる |
| 同じ段の中で、同じパスに違う指令 | ポリシーファイルなら種類 `policy`、コマンドラインなら種類 `usage` の診断 |
| 違う段で同じパスに指令 | 上の段の指令が下の段の指令を置き換える |

「同じ段」は、同じポリシーファイルの中か、コマンドラインの中である。
たとえばプロファイルの `hide` とコマンドラインの `--rw` が同じ実体を指せば、`rw` が残る。
同じプロファイルの中で `rw` と `hide` が同じ実体を指せば、種類 `policy` の診断で終わる。

## 合成した後の検査
<!-- @kotowari[REQ-155:c0592024, REQ-157:73564ea8, EX-359:3220c434, EX-362:e7e5c6bc, EX-363:265a58bd] -->

段を合成した結果について、次の 2 つを検査する。
どちらもポリシーの読み込みに失敗し、種類 `policy` の診断で終わる。

| 条件 | 理由 |
|---|---|
| `env.set` と `secrets` に同じキーがある | 1 つの変数に 2 つの値を入れようとしている |
| `env.mode` が `inherit` なのに `env.pass` が空でない | `pass` は `clear` のときだけ意味を持ち、書いても効かない |

検査するのは合成後の値である。
下の段が `mode = "inherit"` で、上の段が `mode = "clear"` と `pass` を両方書く形は通る。

```toml
# プロファイル
[env]
mode = "inherit"
```

```toml
# --policy-file（通る）
[env]
mode = "clear"
pass = ["LANG", "TERM"]
```

## 同梱プロファイル
<!-- @kotowari[REQ-364:5d9b86a5, REQ-365:adbf7e87, EX-668:a25e360a, EX-669:32ebabe3, EX-670:8abcc2dc, EX-671:05b0e052] -->

同梱プロファイル `examples/profile/default.toml` は、WSL2 向けの出発点である。
組み込みの既定はこれと同じ内容で、`kakoi init` がファイルとして書き出す（[コマンドライン](03-cli.md)）。
README から参照している。

中身は既存の codex jail（kakoi 以前に使っていた bash スクリプト）のマウント表を転記したもので、パスは元の記法のまま書き、リンクを実体へ書き直していない。
次のものを含む。

| 対象 | 内容 |
|---|---|
| `mounts.rw` | `${workspace}`、`${worktree}`、`${git_common_dir}`、`/tmp/kakoi` |
| `mounts.hide` | `/tmp`、`/run/user` |
| `mounts.scan` | ワークツリーの中の `.env` と `.env.*` を隠す走査 |
| `mounts.hide-mounts` | `/mnt` の下の `9p` と `drvfs` のマウントを隠す指定 |
| `env.unset` | `SSH_AUTH_SOCK`、`DISPLAY`、`WAYLAND_DISPLAY`、`XAUTHORITY`、`*_TOKEN`、`*_API_KEY`、`*_SECRET*`、`*_PASSWORD`、`AWS_*`、`GH_*`、`GITHUB_*` |
| `secrets` | `${config_dir}/secrets` の下を指すコメント 1 行だけ。有効な項目は無い |

`/tmp` と `/run/user` にはホストの X11 や ssh-agent のソケット（`/run/user` なら D-Bus、gpg-agent、Wayland のソケット）がランダムな名前で置かれるので、名前ではなく丸ごと隠し、`/tmp/kakoi` だけをホストと共有する場所にしている。

`secrets` をコメントにしてあるのは、秘密ファイルをまだ置いていない状態（組み込みの既定での起動を含む）で、起動のたびに「秘密ファイルが無い」という警告を出さないためである。
秘密を渡すときは、利用者がコメントを外し、ファイルを置く。

```toml
[secrets]
# GH_TOKEN = "${config_dir}/secrets/gh-token"
```

上の表にない項目（パッケージのキャッシュ、エージェント CLI の設定ディレクトリ、認証情報のディレクトリ）の一覧は、同梱プロファイルのファイルそのものを参照する。
