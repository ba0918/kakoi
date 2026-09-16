# ポリシーの入力と合成

既存仕様から継承した本体の検査用表現。同じ責務を一文書にまとめ、見出しと要求IDで参照する。正本は責務別spec文書群。

## 要求

### REQ-151: 固定キー・TOML・必須値
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

TOML。固定キーは mounts の rw・rw-file・rw-copy・ro・hide・scan・hide-mounts、scan の root・names・exclude・prune、hide-mounts の under・fstype、env の mode・pass・set・unset・path-prepend、secrets、git.instead-of とする。

network と process の追加キーはネットワーク IR で定める。固定のキー名について、これ以外の固定キーはポリシー読み込み失敗。TOML として解析できないファイルもポリシー読み込み失敗。"env.set"、"secrets"、"git.instead-of" の 3 つのテーブルだけは、キー名が利用者の書く値である。

この例は形式を示すためのもので、値は例である。同梱する "examples/profile/default.toml" の値は第 16 節で定める。セクションは省略できる。空のファイルは有効なポリシーファイルである。"mounts.scan" の項目では"root" と "names" が、"mounts.hide-mounts" の項目では "under" と "fstype" が必須で、欠けているか空ならポリシー読み込み失敗。

### REQ-152: パス記法と Git の相互参照
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

ポリシーファイルに書くパスを取る値（"mounts" の 5 指令、"scan.root"、"hide-mounts.under"、"secrets" の値、"env.path-prepend"）は、絶対パス、"~" 単独、"~/" で始まるパス、変数で始まるパスのどれかで書く。相対パスと "~ユーザー名" の形はポリシー読み込み失敗。

"~" はホームディレクトリ（第 2 節。"HOME" の実体のパス）に展開する。字面の "HOME" は使わない。"HOME" の検査は第 2 節のとおりで、第 13 節の段階 4 で常に行う。変数は 4 つ。値はすべて実体のパスで、変数の導出時に解決する。設定ディレクトリが存在しなければ"${config_dir}" は値を持たない（組み込みの既定で起動したときに起きる。第 5.3 節）。

設定ディレクトリが存在するのに実体のパスを得られない（途中が辿れない、実体がディレクトリでない）ときは種類 "path" の診断で終わる。表 TBL-151 に従う。"${git_common_dir}" は次のとおり決める。ワークツリーの ".git"（シンボリックリンクを辿らずに見る）がディレクトリならそのディレクトリ。

通常ファイルなら、その中の "gitdir:" が指すディレクトリを G とし、次のどちらかが成り立つ場合だけ値を持つ。

リンクされたワークツリー: G の "commondir" ファイルの内容を G からの相対で解決したものを C とする。  G が "C/worktrees/" の直下にあり、かつ "G/gitdir" ファイルの内容がワークツリーの ".git" ファイルを  指している。このとき値は C。

サブモジュール: G に "commondir" が無く、G の "config" ファイルの "core.worktree" が G からの相対で  ワークツリーを指している。このとき値は G。リンクされたワークツリーの形では、さらに C の直下に通常ファイル "HEAD" が存在しなければ相互リンクの不成立とする（"HEAD" 自身がシンボリックリンクなら辿らず、内容は読まない）。

それ以外は種類 "path" の診断で終わる。これらの比較はすべて実体のパスで行う。どちらの相互リンクも git 自身が作るものである。ただし C は G の場所だけで決まるので、隔離の中から書ける場所を G にして "commondir" を上へ向ける偽造（ワークツリーやその親の名前が "worktrees" のとき、または "rw" の項目の名前や親の名前が "worktrees" のとき）は ".git" ファイルとワークツリーの中身だけで組める。

"HEAD" の要求がこれを止める: "C/HEAD" を隔離の中から作れるなら C は既に書き込める場所で、新しく露出するものが無い。"HEAD" が既にある書き込めないC の "worktrees/" の直下に書き込める項目がある配置は、git も第 16 節の同梱プロファイルも作らない。".git" と G の下で読むファイル（"commondir"、"gitdir"、"config"）は、開くファイル自身がシンボリックリンクなら辿らない（解決の途中のリンクは辿る）。

通常ファイルでないものは相互リンクの不成立と同じく種類 "path" の診断で終わる。git 自身はこれらをシンボリックリンクで作らず、G は隔離の中から書ける領域にありうるためである。読む長さの上限は第 14 節。

### REQ-153: 変数を展開できない場合と対象外の値
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

値を持たない変数を含む項目は、存在しないパスと同じ扱いで飛ばす。マウント項目、走査の "root"、"hide-mounts" の "under"、"env.path-prepend" の項目のどれでも、飛ばしたことは計画に理由付きで表示する。"secrets" の値がそうなったときは、存在しない秘密ファイルとして第 9 節の警告になる。

未知の変数名はポリシー読み込み失敗。変数展開はパスを取る値にだけ効く。"env.set" の値には効かない。これらの値は git コマンドを実行せずファイルシステムから導出する。ワークスペースが存在しないか、実体がディレクトリでなければ、種類 "path" の診断で終わる。成功の観測条件: 値を持つ変数がすべて実体のパスで計画に表示され、値を持たない変数はそう表示され、"--workspace" に通常ファイルを渡すと終了コード 125 の "path" で終わる。

反例: "${config_dir}" だけが "XDG_CONFIG_HOME" の字面で表示される。

### REQ-154: 選択するプロファイルと合成の優先順位
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

書かれた段は最大 3 つ。下から、"--profile"（省略時 "default"）、"--policy-file"、コマンドライン。"--profile" で "default" 以外を指定したとき "default.toml" は読まない。指定したプロファイルのファイルが無ければポリシー読み込み失敗。

ただし "--profile" を省略したか "default" を明示したときに"profile/default.toml" が無ければ、組み込みの既定（第 2 節）がグローバルスコープになる。「無い」とは、組み立てたパス "<設定ディレクトリ>/profile/default.toml" の各成分を根から順に見て（設定ディレクトリの祖先を含む）、最初に失敗した成分が「名前が存在しない」（リンクを辿らずに見て無い）ことである。

最初に失敗した成分がリンク切れのシンボリックリンク、またはディレクトリの場所にある通常ファイルなら、どの成分でも「ある」に数え、辿れないのでポリシー読み込み失敗になる。全成分が辿れて "default.toml" が読めないときもポリシー読み込み失敗である。厳しいプロファイルが壊れたときに、黙って広い既定で動かないためである。

無いディレクトリは新しい機械の普通の状態なので「無い」に数える。"XDG_CONFIG_HOME" にまだ clone していない dotfiles の中を指している間は組み込みの既定で動くことになるので、README に書く。組み込みの既定は段を増やさず、ファイルの代わりにグローバルスコープの中身になるだけで、その上に "--policy-file" とコマンドラインが同じ規則で重なる。

"default" 以外の名前を指定して無いときに組み込みの既定へ落ちることはない（名前を指定した利用者はそのファイルがあると思っている）。組み込みの既定で起動しても、起動のたびに警告や通知は出さない。"--print-plan" の使ったポリシーファイルの欄が、組み込みの既定を使ったことを文字列 "kakoi init" で示す（第 13 節）。

成功の観測条件: 設定ディレクトリが無い状態で "--print-plan" すると終了コード 0 で、計画が組み込みの既定を使ったことを示し、同じ状態で "--profile strict --print-plan" は種類 "policy" の診断で終わる。"profile/default.toml" を置くとそれだけが読まれ、組み込みの既定は使われない。

設定ディレクトリが無く"--policy-file" を与えると、組み込みの既定の上にそれが重なり、その "--policy-file" が "${config_dir}" を参照する "ro" 項目を書いていれば、その項目は値を持たない変数として飛ばされ理由付きで計画に出る。"profile/default.toml" か設定ディレクトリがリンク切れのシンボリックリンクなら種類 "policy" の診断で終わる。

反例: "--profile strict" が無いときに黙って組み込みの既定で起動する。組み込みの既定と "default.toml" が同時に読まれる。リンク切れの "default.toml" で組み込みの既定が使われる。合成規則は値の種類ごとに決まる。表 TBL-152 に従う。"env.path-prepend" の連結は上の段の項目が先頭側に来る。

"PATH" に足したとき、上の段の項目ほど前に置かれる。

### REQ-155: 削除を持たない合成とワイルドカード
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

削除の演算子は無い。下の段の項目を上の段で消すことはできない。テーブルのキーを「無し」に戻すこともできない。それが必要なら別のプロファイルを作る。合成後に "env.set" と "secrets" に同じキーがあれば、ポリシー読み込み失敗。"env.unset" の項目はシェルのワイルドカード（"*"、"?"）を使える。

ワイルドカードの意味は本仕様の全体（"env.unset"、第 6.3 節の走査の "names"・"exclude"・"prune"）で共通で、"*" は 0 文字以上の任意の並び（名前の先頭の "." にも一致する）、"?" は任意の 1 バイト、それ以外の文字はすべて字面どおりに一致する。文字クラスやエスケープは無い。

### REQ-156: 実体による同一性と段ごとの競合
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

"mounts" の 5 指令の項目は、チルダと変数を展開し、シンボリックリンクを解決した実体のパスで同一性を判定する。実体が存在しないパスは、チルダと変数を展開した後の文字列で判定する（書いたままの字面ではない）。同じ段の中（同じポリシーファイルの中、またはコマンドラインの中）で、同じパスに同じ指令が 2 回現れたら 1 つにまとめる。

同じパスに違う指令が現れたら、ポリシー読み込み失敗（コマンドラインなら種類 "usage"）。段をまたいで同じパスに指令が現れたら、上の段の指令が下の段の指令を置き換える。

### REQ-157: 合成後に効かない env.pass の拒否
- 種類: ubiquitous
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- 検証: unit

合成後の "env.mode" が "inherit" のとき "env.pass" が空でないなら、ポリシー読み込み失敗。下の段が"inherit" で、上の段が "clear" と "pass" を両方書く形は通る。

## 決定表

### TBL-151: パス変数の値
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1

| 変数 | 値 |
|---|---|
| "${workspace}" | ワークスペースの実体のパス |
| "${worktree}" | ワークツリーの実体のパス |
| "${git_common_dir}" | REQ-152 で検証した共有 ".git" の実体のパス。ワークツリーに ".git" が無ければ値を持たない |
| "${config_dir}" | 設定ディレクトリの実体のパス。設定ディレクトリが存在しなければ値を持たない |

### TBL-152: 段の合成
- 出典: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1

| 種類 | 対象 | 規則 |
|---|---|---|
| リスト | "mounts.rw" "rw-file" "rw-copy" "ro" "hide" "scan" "hide-mounts"、"env.pass" "unset" "path-prepend" | 連結。上の段が下の段に足す |
| スカラー | "network.mode"、"env.mode" | 上の段が上書き |
| テーブル | "env.set"、"secrets"、"git.instead-of" | キー単位でマージ。同じキーは上の段が勝つ |

## 具体例

```gherkin
@id=EX-360 @about=REQ-156 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 上の段が勝つ
  Given 下の段の hide と上の段の rw が同じ実体を指す
  When 同一性を判定する
  Then 上の段の rw が残る
```

```gherkin
@id=EX-361 @about=REQ-156 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 同じ段の競合
  Given 同じ段の rw と hide が同じ実体を指す
  When 同一性を判定する
  Then ポリシーファイルなら policy の診断で終了する
```

```gherkin
@id=EX-362 @about=REQ-157 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 有効な pass
  Given 下の段は inherit で上の段は clear と pass を持つ
  When ポリシーを合成する
  Then pass を受け入れる
```

```gherkin
@id=EX-363 @about=REQ-157 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 効かない pass
  Given 合成後は inherit で pass が空でない
  When 合成結果を検査する
  Then policy の診断で終了する
```

```gherkin
@id=EX-350 @about=REQ-151 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 空のポリシー
  Given 空の TOML ファイル
  When そのファイルを読む
  Then ポリシーとして受け入れる
```

```gherkin
@id=EX-351 @about=REQ-151 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 未知の固定キー
  Given 固定キー mount = [] を持つ TOML
  When そのファイルを読む
  Then policy の診断で終了する
```

```gherkin
@id=EX-352 @about=REQ-152 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: ホームの実体
  Given HOME が /home/link で実体は /home/u
  When ~/cache を展開する
  Then /home/u/cache を得る
```

```gherkin
@id=EX-353 @about=REQ-152 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: Git 相互参照の欠落
  Given リンクされたワークツリーの共有ディレクトリに通常ファイル HEAD がない
  When git_common_dir を求める
  Then path の診断で終了する
```

```gherkin
@id=EX-354 @about=REQ-153 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 値のない変数
  Given git_common_dir が値を持たない
  When その変数で始まる rw を処理する
  Then 理由付きで飛ばして計画へ残す
```

```gherkin
@id=EX-355 @about=REQ-153 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 展開しない値
  Given env.set に ${worktree} という値がある
  When ポリシーのパスを展開する
  Then env.set の値は文字列のまま残る
```

```gherkin
@id=EX-356 @about=REQ-154 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 既定の代替
  Given 既定プロファイルの名前が存在しない
  When default で起動する
  Then 組み込みの既定をグローバルスコープに使う
```

```gherkin
@id=EX-357 @about=REQ-154 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 壊れた既定
  Given 既定プロファイルへの経路にリンク切れがある
  When default で起動する
  Then policy の診断で終了して組み込みへ切り替えない
```

```gherkin
@id=EX-358 @about=REQ-155 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 先頭ドット
  Given ファイル名が .env
  When パターン * と照合する
  Then 一致する
```

```gherkin
@id=EX-359 @about=REQ-155 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 注入先の競合
  Given 合成後の env.set と secrets に同じキーがある
  When ポリシーを合成する
  Then policy の診断で終了する
```
