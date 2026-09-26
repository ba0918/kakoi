# 計画の表示

この章は、`--print-plan` が表示する計画の中身を、要約（`summary`）、全量（`full`）、JSON（`json`）の形式ごとに定める。
あわせて、計画を出す前のマウントの検査で、複数の誤りがあるときにどの診断が出るかを定める。
起動前にどのマウントと環境で動くかを確かめるとき、ツールから計画を読むときに読む。

**計画**は、ポリシーと収集した事実から算出した、bwrap の引数列、環境、各マウント項目の適用結果、解決したコマンドのパスの組である（[用語](02-terms.md)）。
`--print-plan` の書き方と `FORM` の指定の規則は[コマンドライン](03-cli.md)が扱う。
`--print-plan` は bwrap を起動する直前までの検査をすべて行い、診断があれば計画を出さずにその診断で終わる。
成功すれば計画を標準出力へ出して終了コード 0 で終わる（[入れ子、並列、出力、診断と終了コード](10-process.md)）。

## 形式の選び方

| FORM | 読み手 | 中身 | 契約になる範囲 |
|---|---|---|---|
| `summary`（省略時） | 人 | 起動に関わる事実の要約 | 表示する項目。レイアウトと文言は含まない |
| `full` | 人 | 合成後のポリシー、出所、最終の環境の全体、bwrap の引数列 | 表示する項目。レイアウトと文言は含まない |
| `json` | LLM エージェントとツール | 全量に環境の差分を加えたもの | キーと値の意味 |

この章の出力例は現行の版で観測したもので、ホームディレクトリを `/home/alice` に置き換えてある。
例は次のプロファイルを `profile/default.toml` に置き、`~/proj` から実行した。

```toml
[mounts]
rw = ["${worktree}", "~/.cache", "~/.npm"]
rw-copy = ["~/.gitconfig"]
hide = ["/tmp", "~/.ssh"]

[network]
mode = "none"

[env]
mode = "inherit"
set = { EDITOR = "vi" }
unset = ["SSH_AUTH_SOCK"]

[secrets]
GH_TOKEN = "${config_dir}/secrets/gh-token"
```

## 要約（summary）の中身
<!-- @kotowari[REQ-296:0924dc17, EX-532:29594d6e] -->

要約は `--print-plan` の省略時の形式で、次の 6 つを表示する。

| 欄 | 表示する内容 |
|---|---|
| ポリシーファイル | 使ったポリシーファイルの実体のパス。組み込みの既定を使ったときは、この欄に文字列 `kakoi init` を含む |
| 変数 | 4 つの変数（`workspace`、`worktree`、`git_common_dir`、`config_dir`）の値 |
| ネットワーク | 通信モード |
| マウント | 各項目を適用したか飛ばしたか、飛ばした理由 |
| 環境 | 最終の環境のホストとの差分 |
| コマンド | 解決したコマンドのパス |

組み込みの既定を使ったことを示す文字列 `kakoi init` だけは契約で、その前後の文言は契約に含めない。
利用者が次に何をすればプロファイルを置けるかが、この欄から分かるようにしている。

マウントの欄では、ホームディレクトリのパスを `~` に縮めて表示する。
グローバルスコープ以外から来た項目にだけ、出所を添える（例の `(secret GH_TOKEN)`）。

環境の欄は、ホストとの差分を次のように表示する。

- 消した変数は名前を出す。
- 設定した変数は名前と値を出す。
- 秘密は名前だけを出し、値を伏せる。
- ホストと同じ値のまま渡す変数は、件数だけを出す。

ホストと同じ `KEPT` と追加した `NEW` があれば、`KEPT` は件数に含まれ、`NEW` はその値が表示される。

```text
policy files:
  /home/alice/.config/kakoi/profile/default.toml
variables:
  workspace = /home/alice/proj
  worktree = /home/alice/proj
  git_common_dir = /home/alice/proj/.git
  config_dir = /home/alice/.config/kakoi
network: none
mounts (~ is /home/alice):
  hide    /tmp
  rw      ~/.cache
  hide    ~/.config/kakoi/secrets (secrets/ of the configuration directory)
  hide    ~/.config/kakoi/secrets/gh-token (secret GH_TOKEN)
  rw-copy ~/.gitconfig
  rw      ~/proj
  skipped rw `~/.npm`: does not exist
  skipped hide `~/.ssh`: does not exist
  (rw-copy starts from a copy of the host's content and is writable inside; nothing written there reaches the host, and it is gone when the command ends)
environment (inherit): 3 variables as on the host, and:
  unset  SSH_AUTH_SOCK
  set    EDITOR=vi
  set    KAKOI=1
  secret GH_TOKEN (value not shown)
command: /usr/bin/sh
bwrap: /usr/bin/bwrap
(--print-plan=full adds the merged policy, the origin of every item, the whole environment, and the bwrap arguments; --print-plan=json is the same as one line of JSON, for LLM agents and tools)
```

組み込みの既定で起動したときは、ポリシーファイルの欄が次のようになる。

```text
policy files:
  the built-in default (write it out with `kakoi init`)
```

## 全量（full）の中身
<!-- @kotowari[REQ-297:94502576, EX-533:0e201d00] -->

全量は `--print-plan=full` で表示し、要約の欄に代えて次の 5 つを表示する。

| 欄 | 表示する内容 |
|---|---|
| 合成後のポリシー | 段を合成した後のポリシーの全体と、各項目の出所 |
| マウント | 各マウント項目の実体のパスと出所 |
| 環境 | 最終の環境の全体。秘密の値は伏せる |
| コマンド | 解決したコマンドのパス |
| bwrap の引数列 | bwrap に渡す引数。ファイル記述子の位置は記号で示す |

```text
policy (merged):
  mounts.rw `${worktree}` (from the profile /home/alice/.config/kakoi/profile/default.toml)
  mounts.rw-copy `~/.gitconfig` (from the profile /home/alice/.config/kakoi/profile/default.toml)
  ...
  secrets GH_TOKEN = `${config_dir}/secrets/gh-token`
mounts:
  hide    /home/alice/.config/kakoi/secrets/gh-token (from the secret `GH_TOKEN`: `/home/alice/.config/kakoi/secrets/gh-token`)
  rw-copy /home/alice/.gitconfig (from the profile /home/alice/.config/kakoi/profile/default.toml: `~/.gitconfig`)
  ...
environment:
  EDITOR=vi
  GH_TOKEN=<secret, not shown>
  HOME=/home/alice
  ...
command: /usr/bin/sh
bwrap: /usr/bin/bwrap
bwrap arguments:
  --ro-bind
  /
  /
  ...
  --seccomp
  <fd: seccomp filter>
  ...
  --bind-data
  <fd: copied file, 1 bytes>
  /home/alice/.gitconfig
  ...
```

`...` は例を短くするために省いた行である。

## 要約と全量の両方に出るもの
<!-- @kotowari[REQ-297:94502576, REQ-436:c599c3f3, REQ-450:72b2e13e] -->

次の 5 つは、要約と全量のどちらでも同じように表示する。

- 飛ばした項目とその理由
- `rw-copy` が複製できなかったエントリとその理由
- `rw-copy` の項目があるときの説明（ホストの内容で始まり、書き込みはホストに出ず、終了時に消える）
- 入れ子であることの印
- 見張り役を置いたプログラムとその場所、飛ばしたプログラムとその理由（[コマンドのガードレール](13-command-guard.md#計画表示)）

`rw-copy` の説明を添えるのは、指令の名前だけでは `rw` との違いが読み取れないためである。
入れ子で計画を表示したときの `PATH` とコマンドの欄の違いは[コマンドライン](03-cli.md)を参照する。
全量の形式でこれらが出ることを確かめるテストが足りていない点は[未決事項](../appendix/open-issues.md)に記す。

## JSON の外形と版
<!-- @kotowari[REQ-298:87b1a34f, EX-534:801623fd] -->

`--print-plan=json` は、全量と同じ内容に要約の環境の差分を加えたものを、1 行の JSON 文書として出す。
出力は末尾の改行（LF）1 つで終わる。
人が読むことは想定せず、LLM エージェントとツールが読む形である。
要約と全量と違い、この形式ではキーと値の意味が契約になる。

版はキー `format_version` の整数で示し、1 から始める。

| 変更 | `format_version` |
|---|---|
| キーを消す | 上げる |
| キーの意味を変える | 上げる |
| キーを足す | 上げない |

同じ版の間は、既存のキーをなくさず、意味も変えない。
たとえば `not_copied` と種類 `copied-file` は後から足したキーと値だが、追加なので版は 1 のままである。
読む側は、知らないキーを無視すれば同じ版の間は読み続けられる。

## JSON のキー
<!-- @kotowari[REQ-299:53ae04ca, EX-535:239c4e1a] -->

トップレベルのキーは次の 18 個である。

| キー | 値 |
|---|---|
| `format_version` | 版を表す整数 |
| `nested` | 入れ子かどうか |
| `policy_sources` | 使ったポリシーの情報 |
| `variables` | 4 つの変数。値を持たない変数は `null` |
| `home` | ホームディレクトリ |
| `policy` | 合成後のポリシー |
| `mounts` | 適用する項目。それぞれ実体のパス、種類、書かれた値、出所を持つ |
| `skipped_mounts` | 飛ばしたマウント項目 |
| `left_visible` | 走査の後も見えるままにした対象 |
| `skipped_paths` | 飛ばしたパス |
| `not_copied` | `rw-copy` で複製できなかったもの（[JSON の秘密とファイル記述子](#json-の秘密とファイル記述子)） |
| `guards` | 見張り役を置いたプログラム（[コマンドのガードレール](13-command-guard.md#計画表示)） |
| `skipped_guards` | 規則があって見張り役を置かなかったプログラムと理由 |
| `environment` | 最終の環境。秘密の値は `null` |
| `environment_changes` | 要約と同じ、ホストの環境との差分 |
| `command` | 解決したコマンド。`COMMAND` を省略したときは `null` |
| `bwrap` | bwrap の情報 |
| `bwrap_arguments` | bwrap の引数列（[JSON の秘密とファイル記述子](#json-の秘密とファイル記述子)） |

`COMMAND` を省略して `--print-plan=json` を実行すると、`command` は `null` になり、`variables` は 4 つの変数を持つ。

例は `jq` で整形し、`policy` の中身を `"…"` に置き換え、`mounts` と `bwrap_arguments` の要素を一部に絞ってある。
実際の出力は 1 行である。
表に挙げていない内側のキー（例の `directive` や `written`）は現行の版の形であり、この章では契約として扱わない。

```json
{
  "format_version": 1,
  "nested": false,
  "policy_sources": [
    { "kind": "file", "path": "/home/alice/.config/kakoi/profile/default.toml" }
  ],
  "variables": {
    "workspace": "/home/alice/proj",
    "worktree": "/home/alice/proj",
    "git_common_dir": "/home/alice/proj/.git",
    "config_dir": "/home/alice/.config/kakoi"
  },
  "home": "/home/alice",
  "policy": "…",
  "mounts": [
    {
      "directive": "hide",
      "path": "/home/alice/.config/kakoi/secrets/gh-token",
      "kind": "not-directory",
      "written": "/home/alice/.config/kakoi/secrets/gh-token",
      "origin": { "kind": "secret", "name": "GH_TOKEN" }
    },
    {
      "directive": "rw-copy",
      "path": "/home/alice/.gitconfig",
      "kind": "not-directory",
      "written": "~/.gitconfig",
      "origin": { "kind": "profile", "path": "/home/alice/.config/kakoi/profile/default.toml" }
    }
  ],
  "skipped_mounts": [
    {
      "directive": "rw",
      "written": "~/.npm",
      "origin": { "kind": "profile", "path": "/home/alice/.config/kakoi/profile/default.toml" },
      "reason": "does not exist"
    }
  ],
  "left_visible": [],
  "skipped_paths": [],
  "not_copied": [],
  "guards": [],
  "skipped_guards": [],
  "environment": {
    "EDITOR": "vi",
    "GH_TOKEN": null,
    "HOME": "/home/alice",
    "KAKOI": "1",
    "LANG": "C.UTF-8",
    "PATH": "/usr/bin:/bin"
  },
  "environment_changes": {
    "mode": "inherit",
    "kept": 3,
    "unset": ["SSH_AUTH_SOCK"],
    "set": { "EDITOR": "vi", "KAKOI": "1" },
    "secrets": ["GH_TOKEN"]
  },
  "command": { "given": "sh", "arguments": ["-c", "echo hi"], "path": "/usr/bin/sh" },
  "bwrap": "/usr/bin/bwrap",
  "bwrap_arguments": [
    { "kind": "literal", "value": "--ro-bind" },
    { "kind": "literal", "value": "/" },
    { "kind": "seccomp-filter" },
    { "kind": "empty-file" },
    { "kind": "copied-file", "bytes": 1 }
  ]
}
```

## JSON の秘密とファイル記述子
<!-- @kotowari[REQ-300:b3486598, EX-536:3fc0dfd7] -->

秘密の値はどのキーにも出さない。
`environment` では秘密の変数の値を `null` にし、他のキーにも値を載せない。

`bwrap_arguments` の各要素は `kind` で種類を区別する。
ファイル記述子で渡す引数は、内容の代わりに記述子の種類を示す。

| `kind` | 追加のキー | 表すもの |
|---|---|---|
| `literal` | `value` | 引数の文字列 |
| `seccomp-filter` | 無し | seccomp のフィルタを渡す記述子 |
| `empty-file` | 無し | 空ファイルを渡す記述子 |
| `copied-file` | `bytes` | `rw-copy` で複製する通常ファイルを渡す記述子。長さだけを持ち、内容は出さない |

`not_copied` の各要素は、`rw-copy` の項目の実体のパス、複製できなかったエントリのパス、理由を持つ。

## JSON の出所と文字の表し方
<!-- @kotowari[REQ-301:31ff3684, EX-537:0b71f147] -->

項目の出所は、`kind` が次の 8 つのどれかである。
ファイルから来た出所は `path` を、`secret` は `name` を持つ。

| `kind` | 項目の来たところ |
|---|---|
| `profile` | プロファイル |
| `built-in-default` | 組み込みの既定 |
| `policy-file` | `--policy-file` で指定したポリシーファイル |
| `command-line` | コマンドライン（`--rw`、`--hide`） |
| `scan` | 走査が生成した `hide` |
| `hide-mounts` | `mounts.hide-mounts` が生成した `hide` |
| `secret` | 秘密のファイルを隠すために生成した項目。`name` に秘密の名前 |
| `config-secrets` | 設定ディレクトリの `secrets/` を隠すために生成した項目 |

たとえば秘密から生成したマウントの出所は `{ "kind": "secret", "name": "GH_TOKEN" }` になる。
この 8 つの列挙が `policy_sources` の `kind` にも及ぶかは決まっていない（例では `policy_sources` の `kind` は `file`）。
これは[未決事項](../appendix/open-issues.md)に記す。

文字列は次のように表す。

- UTF-8 として不正なバイト列は置換文字（U+FFFD）で表す。
- 制御文字は JSON のエスケープで表し、生の制御文字は出さない。

## 計画を出す前の検査

`--print-plan` も起動と同じ段階の順に検査し、診断があれば計画を出さない。
段階の全体と、段階 7（マウントの解決）の中の順序は[プロセスと終了コード](10-process.md)が定める。
