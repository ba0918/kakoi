# 実行してよいプログラムを選ぶ

コマンドのモード "listed"（ポリシーに書いたプログラムだけを隔離の中で起動できる形）と、それを Landlock で当てること、当てられないときの扱い、計画の表示、見せるものを選ぶ形の公開文書を扱う。これは境界ではなくガードレールで、止め漏れる形は TBL-160 にある。使い方を選ぶ規則は core-command-guard-rules.md にある。

## Requirements

### REQ-474: コマンドのモードと許すプログラム
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-30-allowlist.md#A30, docs/decision/brainstorm/2026-09-30-allowlist.md#A34, docs/decision/brainstorm/2026-09-30-allowlist.md#A36
- verification: unit

ポリシーの "commands.mode" は "host" か "listed" で、どの段にも書かれていなければ "host" とする。どれかの段が "listed" なら、上の段が "host" でも合成後は "listed" とする。"commands.allow" はパスの一覧で、段をまたいで連結し、上の段が足す。パスの書き方はマウントの項目と同じとする。合成後の "commands.mode" が "listed" でないのに "commands.allow" に項目があるとき、"commands.mode" に "host" と "listed" 以外の値を書いたときは、ポリシー読み込み失敗とする。コマンドのモードはマウントのモードによらず使える。

### REQ-475: 許していないプログラムの起動を止める
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-30-allowlist.md#A13, docs/decision/brainstorm/2026-09-30-allowlist.md#A17, docs/decision/brainstorm/2026-09-30-allowlist.md#A41, docs/decision/brainstorm/2026-09-30-allowlist.md#A45, docs/decision/brainstorm/2026-09-30-allowlist.md#A58
- verification: unit

コマンドのモードが "listed" のとき、bwrap はコマンドの代わりに `隔離の中の最初のプロセス` を起動し、それが Landlock を掛けてからコマンドを起動する。Landlock は "commands.allow" の項目と、kakoiが自動で許すものだけに実行を許す。項目は `隔離の中の最初のプロセス` が隔離の中のパスとして開く。項目がディレクトリならその下のすべてに、ファイルならそのファイルに許す。シンボリックリンクの項目は、辿った先の実体に許す。kakoiが自動で許すのは、動的リンカ "/lib64/ld-linux-x86-64.so.2" の実体と kakoi 自身の実行ファイル（`見張り役` と `隔離の中の最初のプロセス` に使うもの）だけである。許していないプログラムを起動すると、起動は失敗する。コマンドの起動が失敗したときは、`隔離の中の最初のプロセス` がコマンドの名前と理由を示す診断を出して 126 で終わる。`隔離の中の最初のプロセス` が項目を開けなければ、その項目を許さずに進め、標準エラーに警告を1行出す。動的リンカが隔離の中に無ければ、それを許さずに進める。kakoi 自身の実行ファイルの場所が分からなければ、コマンドを実行せず種類 bwrap の診断で 125 とする。

### REQ-476: 許すプログラムがホストに無いとき
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-30-allowlist.md#A22, docs/decision/brainstorm/2026-09-30-allowlist.md#A45
- verification: unit

"commands.allow" の項目のパスがホストに無いとき、隔離の中に見えないときは、REQ-169 のマウントの項目と同じく、その項目を理由を付けて計画に示して飛ばし、起動を止めない。

### REQ-477: Landlock が使えないとき
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-30-allowlist.md#A15, docs/decision/brainstorm/2026-09-30-allowlist.md#A22, docs/decision/brainstorm/2026-09-30-allowlist.md#A35
- verification: unit

コマンドのモードが "listed" で、ホストで Landlock が使えないときは、コマンドを実行せず種類 bwrap の診断で 125 とする。この確かめは bwrap の所在の段階で行い、計画表示でも行う。

### REQ-478: コマンドのモードの計画表示
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-30-allowlist.md#A26, docs/decision/brainstorm/2026-09-30-allowlist.md#A37, docs/decision/brainstorm/2026-09-30-allowlist.md#A45
- verification: unit

計画の要約は、REQ-473 の行とは別の1行で、合成後のコマンドのモードと、"listed" のときは許すプログラムの項目の数を示す。計画の JSON は、合成後のコマンドのモードと許すプログラムの項目を持つ。数と項目は REQ-476 で飛ばした項目を含まない。"format_version" は 1 のままとする。

### REQ-479: 見せるものを選ぶ形の公開文書
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-30-allowlist.md#A13, docs/decision/brainstorm/2026-09-30-allowlist.md#A51, docs/decision/brainstorm/2026-09-30-allowlist.md#A3, docs/decision/brainstorm/2026-09-30-allowlist.md#A10, docs/decision/brainstorm/2026-09-30-allowlist.md#A30, docs/decision/brainstorm/2026-09-30-allowlist.md#A31, docs/decision/brainstorm/2026-09-30-allowlist.md#A25, docs/decision/brainstorm/2026-09-30-allowlist.md#A54
- verification: review
- how_to_verify: "docs/policy.md" の mounts 節と commands 節、"docs/cli.md" の init の説明、"docs/security.md" の既知の隙間を読み、本文の各事項が書かれていることと、TBL-160 の行と一致することを確かめる。

"docs/policy.md" に、"mounts.mode" と "mounts.system"（`土台` の中身と、"listed" で書いていない場所が存在しないこと）、"commands.mode" と "commands.allow"、規則の "only" を書く。コマンドのモードが "listed" の隔離の中では、"--nested=isolate" で中に隔離を作れないことを書く。"docs/cli.md" に init の "--example" を書く。"docs/security.md" に、実行してよいプログラムを選ぶ形がガードレールであることと、その止め漏れと入れ子の制限を TBL-160 に合わせて書く。

## Examples

```gherkin
@id=EX-920 @about=REQ-474 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A30
Scenario: 一度 listed にしたら上の段で戻せず、許すものは足される
  Given プロファイルが "commands.mode" を "listed" にして git を許し、"--policy-file" が "commands.mode" を "host" にして cat を許している
  When 計画を表示する
  Then コマンドのモードは "listed" で、許すプログラムは git と cat になる

@id=EX-921 @about=REQ-474 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A30
Scenario: listed でないのに許すプログラムを書くと読み込めない
  Given どの段も "commands.mode" を書かず、"commands.allow" に1つ書いている
  When コマンドを起動する
  Then ポリシー読み込み失敗で終わり、コマンドを実行しない

@id=EX-922 @about=REQ-475 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A13,docs/decision/brainstorm/2026-09-30-allowlist.md#A17
Scenario: 許したプログラムはシェルから起動できる
  Given コマンドのモードが "listed" で、"/bin/sh" と "/usr/bin/ls" を許している
  When 隔離の中の sh から ls を起動する
  Then ls が動く

@id=EX-923 @about=REQ-475 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A13
Scenario: 許していないプログラムは起動できない
  Given コマンドのモードが "listed" で、"/bin/sh" だけを許している
  When 隔離の中の sh から "/usr/bin/id" を起動する
  Then 起動は失敗する

@id=EX-924 @about=REQ-475 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A17
Scenario: リンクで書いた項目は実体に効く
  Given コマンドのモードが "listed" で、実行ファイルを指すシンボリックリンクだけを許している
  When 隔離の中でリンクを辿った先の実体のパスで起動する
  Then 起動できる

@id=EX-925 @about=REQ-476 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A22
Scenario: ホストに無いプログラムは飛ばす
  Given コマンドのモードが "listed" で、ホストに無いパスを "commands.allow" に書いている
  When 計画を表示する
  Then その項目は理由を付けて飛ばされ、起動は止まらない

@id=EX-926 @about=REQ-476 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A22
Scenario: 飛ばした項目で起動が広がらない
  Given コマンドのモードが "listed" で、ホストに無いパスと "/bin/sh" を書いている
  When 隔離の中の sh から許していないプログラムを起動する
  Then 起動は失敗する

@id=EX-927 @about=REQ-477 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A15,docs/decision/brainstorm/2026-09-30-allowlist.md#A22
Scenario: Landlock の無いホストでは起動しない
  Given ホストで Landlock が使えず、コマンドのモードが "listed"
  When コマンドを起動する
  Then 種類 bwrap の診断で 125 となり、コマンドを実行しない

@id=EX-928 @about=REQ-477 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A35,docs/decision/brainstorm/2026-09-30-allowlist.md#A15,docs/decision/brainstorm/2026-09-30-allowlist.md#A30
Scenario: host のモードでは Landlock を求めない
  Given ホストで Landlock が使えず、どの段も "commands.mode" を書いていない
  When コマンドを起動する
  Then コマンドを実行する

@id=EX-929 @about=REQ-478 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A26,docs/decision/brainstorm/2026-09-30-allowlist.md#A37
Scenario: 要約と JSON にコマンドのモードが出る
  Given コマンドのモードが "listed" で、プログラムを2つ許している
  When 要約と JSON の計画をそれぞれ表示する
  Then 要約の1行に "listed" と2が、JSON にモードと2つの項目が出て、"format_version" は 1 である

@id=EX-930 @about=REQ-478 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A26,docs/decision/brainstorm/2026-09-30-allowlist.md#A30
Scenario: 書かなければ host と出る
  Given どの段も "commands.mode" を書いていない
  When 要約の計画を表示する
  Then コマンドのモードの行は "host" を示し、項目の数を示さない

@id=EX-940 @about=REQ-479 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A51,docs/decision/brainstorm/2026-09-30-allowlist.md#A13,docs/decision/brainstorm/2026-09-30-allowlist.md#A30,docs/decision/brainstorm/2026-09-30-allowlist.md#A3,docs/decision/brainstorm/2026-09-30-allowlist.md#A54
Scenario: 公開文書で絞り方と止め漏れが分かる
  Given 公開文書を読む
  When コマンドのモードとマウントのモードの説明と、既知の隙間を探す
  Then 書き方、`土台` の中身、入れ子の制限、止め漏れる形が見つかる

@id=EX-941 @about=REQ-479 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A13
Scenario: 公開文書がガードレールを境界と呼ばない
  Given 公開文書を読む
  When コマンドのモードの説明を読む
  Then 境界と書いていれば契約違反である

@id=EX-947 @about=REQ-475 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A41
Scenario: 許していないコマンドを直接起動すると 126 で終わる
  Given コマンドのモードが "listed" で、"/usr/bin/id" を許していない
  When "kakoi -- /usr/bin/id" で起動する
  Then コマンドの名前と理由を示す診断を出して 126 で終わる

@id=EX-948 @about=REQ-475 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A41,docs/decision/brainstorm/2026-09-30-allowlist.md#A45
Scenario: 土台の外の kakoi でも最初のプロセスが動く
  Given kakoi がホームの下にあり、マウントのモードとコマンドのモードが "listed" で、kakoi の場所を書いていない
  When 許したコマンドを起動する
  Then コマンドを実行する
```
