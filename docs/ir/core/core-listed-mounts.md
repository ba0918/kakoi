# 見せるものを選ぶマウント

マウントのモード "listed"（土台とポリシーに書いた場所だけを隔離の中に見せる形）と、それを支える隔離専用の "/tmp"、名前解決の設定、抽象 UNIX ソケットの遮断、計画の表示を扱う。今までの形（マウントのモード "host"）は core-mounts.md にある。

## Requirements

### REQ-467: マウントのモード
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-30-allowlist.md#A1, docs/decision/brainstorm/2026-09-30-allowlist.md#A4, docs/decision/brainstorm/2026-09-30-allowlist.md#A7, docs/decision/brainstorm/2026-09-30-allowlist.md#A10, docs/decision/brainstorm/2026-09-30-allowlist.md#A36, docs/decision/brainstorm/2026-09-30-allowlist.md#A50
- verification: unit

ポリシーの "mounts.mode" は "host" か "listed" で、どの段にも書かれていなければ "host" とする。どれかの段が "listed" なら、上の段が "host" でも合成後は "listed" とする。"mounts.system" は真偽値で、どの段にも書かれていなければ true とする。どれかの段が false なら、上の段が true でも合成後は false とする。"mounts.mode" に "host" と "listed" 以外の値を書いたとき、"mounts.system" に真偽値以外を書いたとき、合成後の "mounts.mode" が "listed" でないのに "mounts.system" が false のときは、ポリシー読み込み失敗とする。

### REQ-468: "listed" で見える場所
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-30-allowlist.md#A3, docs/decision/brainstorm/2026-09-30-allowlist.md#A5, docs/decision/brainstorm/2026-09-30-allowlist.md#A12, docs/decision/brainstorm/2026-09-30-allowlist.md#A42, docs/decision/brainstorm/2026-09-30-allowlist.md#A43, docs/decision/brainstorm/2026-09-30-allowlist.md#A52, docs/decision/brainstorm/2026-09-30-allowlist.md#A55
- verification: unit

マウントのモードが "listed" のとき、隔離の中のファイルシステムは、合成後の "mounts.system" が true なら `土台` の各ディレクトリを読み取り専用で見せ、"rw"、"rw-file"、"rw-copy"、"ro" の項目を "host" のときと同じ見え方で見せる。これ以外のパスは、隔離の中に存在しない。`見せた場所` の中の "hide" と走査は "host" のときと同じに隠す。`見せた場所` の外に向いた "hide"（書いたものと、秘密や走査から生成したもの）、`見せた場所` の外を根とする走査、`見せた場所` の外を "under" とする "hide-mounts" は、重ねずに理由を付けて計画に示して飛ばす。見張り役が包む本物（REQ-446）は、その場所と実体が隔離の中に見えているものに限る。`土台` のうちホストに無いディレクトリは、REQ-169 のとおり理由を付けて飛ばす。項目に書いたパスから辿った先の実体までにあるシンボリックリンクは、同じ行き先の文字列を持つ読み取り専用のリンクとして隔離の中に作り直す。"/dev" と "/proc" は "host" のときと同じく隔離用のものを用意する。

### REQ-469: "listed" の根と親のディレクトリ
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-30-allowlist.md#A5, docs/decision/brainstorm/2026-09-30-allowlist.md#A20, docs/decision/brainstorm/2026-09-30-allowlist.md#A43, docs/decision/brainstorm/2026-09-30-allowlist.md#A49
- verification: unit

マウントのモードが "listed" のとき、`見せた場所` の祖先のうち、ほかの `見せた場所` で覆われていないものを、`見せた場所` とその祖先だけを含むディレクトリとして作る。隔離の根と、こうして作った祖先のディレクトリは読み取り専用とする。隔離の中で書けるのは、"rw"、"rw-file"、"rw-copy" の項目、`見せた場所` の中の "hide" で隠したディレクトリ、REQ-470 の "/tmp"、"/dev" の中で隔離用に用意したものだけである。

### REQ-470: "listed" の "/tmp"
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-30-allowlist.md#A8, docs/decision/brainstorm/2026-09-30-allowlist.md#A49
- verification: unit

マウントのモードが "listed" のとき、kakoiは隔離の中の "/tmp" に、中身の無い書けるディレクトリを隔離専用に用意し、ホストの "/tmp" の中身を見せない。ポリシーが "/tmp" の下の場所（"/tmp/kakoi"）を "rw" に書いていれば、その場所は用意した "/tmp" の上にホストのものとして見える。ポリシーが "/tmp" そのものを項目に書いたときは、その項目の見え方が用意した "/tmp" に勝つ。

### REQ-471: "listed" の名前解決の設定
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-30-allowlist.md#A9, docs/decision/brainstorm/2026-09-30-allowlist.md#A47, docs/decision/brainstorm/2026-09-30-allowlist.md#A55
- verification: unit

マウントのモードが "listed" で合成後の "mounts.system" が true のとき、ホストの "/etc/resolv.conf" が `土台` の外を指すシンボリックリンクなら、kakoiはリンクを辿った先のファイル1つだけを、隔離の中の同じパスに読み取り専用で見せる。そのファイルを含むディレクトリのほかの中身は見せない。リンクを辿った先がホストに無いときは、REQ-169 のとおり理由を付けて飛ばす。"/etc" の下のほかのリンクは扱わない。ネットワークのモードが filtered のときは "/etc/resolv.conf" を差し替えるので、このファイルを見せない。

### REQ-472: "listed" と抽象 UNIX ソケット
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-30-allowlist.md#A23, docs/decision/brainstorm/2026-09-30-allowlist.md#A35
- verification: unit

マウントのモードが "listed" でネットワークのモードが "host" のとき、kakoiは隔離の中のプロセスに Landlock の scope を掛け、隔離の外で作られた抽象 UNIX ソケットへの接続を止める。ホストの Landlock が抽象 UNIX ソケットの scope（ABI 6）を持たないときは、コマンドを実行せず種類 bwrap の診断で 125 とする。この確かめは bwrap の所在の段階で行い、計画表示でも行う。ネットワークのモードが "none" と filtered のときは scope を掛けない。

### REQ-483: "listed" の作業ディレクトリ
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-30-allowlist.md#A44
- verification: unit

マウントのモードが "listed" のとき、作業ディレクトリの実体がどの `見せた場所` の中にも無ければ、種類 "path" の診断で終わる。この検査は計画表示でも行う。

### REQ-473: マウントのモードの計画表示
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-30-allowlist.md#A11, docs/decision/brainstorm/2026-09-30-allowlist.md#A37
- verification: unit

計画の要約は、合成後のマウントのモードと "mounts.system" の値を1行で示す。計画の JSON は、合成後のマウントのモードと "mounts.system" の値を持つ。"format_version" は 1 のままとする。

## Examples

```gherkin
@id=EX-902 @about=REQ-467 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A4,docs/decision/brainstorm/2026-09-30-allowlist.md#A7
Scenario: 一度 listed にしたら上の段で戻せない
  Given プロファイルが "mounts.mode" を "listed" に、"--policy-file" が "host" にしている
  When 計画を表示する
  Then マウントのモードは "listed" になる

@id=EX-903 @about=REQ-467 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A36
Scenario: 知らないモードの値は読み込めない
  Given プロファイルが "mounts.mode" を "clear" にしている
  When コマンドを起動する
  Then ポリシー読み込み失敗で終わり、コマンドを実行しない

@id=EX-904 @about=REQ-467 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A7
Scenario: 一度外した土台は上の段で戻せない
  Given プロファイルが "mounts.system" を false に、"--policy-file" が true にしている
  When 計画を表示する
  Then "mounts.system" は false になる

@id=EX-905 @about=REQ-468 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A3,docs/decision/brainstorm/2026-09-30-allowlist.md#A5
Scenario: 書いていないソケットは存在しない
  Given ホストの "/run" にソケットがあり、マウントのモードが "listed" で "/run" を書いていない
  When 隔離の中でそのソケットのパスを調べる
  Then パスが存在しない

@id=EX-906 @about=REQ-468 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A3,docs/decision/brainstorm/2026-09-30-allowlist.md#A5
Scenario: 土台と書いた場所は見える
  Given マウントのモードが "listed" で、ホームの下の1つのディレクトリを "ro" に書いている
  When 隔離の中で "/usr/bin" のプログラムを起動し、そのディレクトリのファイルを読む
  Then 起動でき、ファイルを読めて、ホームのほかの場所は存在しない

@id=EX-907 @about=REQ-468 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A5
Scenario: 見せた場所の中の hide は効く
  Given マウントのモードが "listed" で、あるディレクトリを "ro" に、その中のファイルを "hide" に書いている
  When 隔離の中でそのファイルを読む
  Then 中身は空に見える

@id=EX-908 @about=REQ-468 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A3
Scenario: 土台を外すと土台も見えない
  Given マウントのモードが "listed" で "mounts.system" が false、"/usr" を書いていない
  When 計画を表示する
  Then "/usr" は隔離の中に見せない

@id=EX-909 @about=REQ-468 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A12
Scenario: ホストに無い土台は飛ばす
  Given ホストに "/lib64" が無く、マウントのモードが "listed"
  When 計画を表示する
  Then "/lib64" は理由を付けて飛ばされ、起動は止まらない

@id=EX-910 @about=REQ-469 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A5,docs/decision/brainstorm/2026-09-30-allowlist.md#A20
Scenario: 親のディレクトリには見せたものだけがあり書けない
  Given マウントのモードが "listed" で、ホームの下の1つのディレクトリだけを "ro" に書いている
  When 隔離の中でホームの一覧を取り、ホームに新しいファイルを作る
  Then 一覧にはそのディレクトリだけがあり、ファイルは作れない

@id=EX-911 @about=REQ-469 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A20
Scenario: 書けると書いた場所には書ける
  Given マウントのモードが "listed" で、ワークスペースを "rw" に書いている
  When 隔離の中でワークスペースにファイルを作る
  Then ファイルを作れて、ホストのワークスペースに残る

@id=EX-912 @about=REQ-470 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A8
Scenario: 隔離専用の /tmp に書ける
  Given ホストの "/tmp" にファイルがあり、マウントのモードが "listed" で "/tmp" を書いていない
  When 隔離の中で "/tmp" の一覧を取り、"/tmp" にファイルを作る
  Then 一覧は空で、ファイルを作れて、ホストの "/tmp" には現れない

@id=EX-913 @about=REQ-470 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A8
Scenario: /tmp の下の共有の場所はホストと共有する
  Given マウントのモードが "listed" で "/tmp/kakoi" を "rw" に書いている
  When 隔離の中で "/tmp/kakoi" にファイルを作る
  Then ホストの "/tmp/kakoi" にファイルが現れる

@id=EX-914 @about=REQ-471 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A9
Scenario: 土台の外を指す resolv.conf を読める
  Given ホストの "/etc/resolv.conf" が "/mnt/wsl/resolv.conf" を指し、マウントのモードが "listed" でネットワークのモードが "host"
  When 隔離の中で "/etc/resolv.conf" を読む
  Then ホストと同じ中身が読める

@id=EX-915 @about=REQ-471 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A9
Scenario: resolv.conf の指す先のほかのファイルは見えない
  Given ホストの "/etc/resolv.conf" が "/mnt/wsl/resolv.conf" を指し、"/mnt/wsl" にほかのファイルがあり、マウントのモードが "listed"
  When 隔離の中で "/mnt/wsl" の一覧を取る
  Then "resolv.conf" だけがある

@id=EX-916 @about=REQ-472 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A23
Scenario: host のネットワークでも外の抽象ソケットに繋がらない
  Given ホストで抽象 UNIX ソケットが待ち受けていて、マウントのモードが "listed" でネットワークのモードが "host"
  When 隔離の中からそのソケットに繋ぐ
  Then 接続は拒否される

@id=EX-917 @about=REQ-472 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A23,docs/decision/brainstorm/2026-09-30-allowlist.md#A35
Scenario: scope の無いホストでは起動しない
  Given ホストの Landlock が抽象 UNIX ソケットの scope を持たず、マウントのモードが "listed" でネットワークのモードが "host"
  When コマンドを起動する
  Then 種類 bwrap の診断で 125 となり、コマンドを実行しない

@id=EX-918 @about=REQ-473 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A11,docs/decision/brainstorm/2026-09-30-allowlist.md#A37
Scenario: 要約と JSON にマウントのモードが出る
  Given プロファイルが "mounts.mode" を "listed" に、"mounts.system" を false にしている
  When 要約と JSON の計画をそれぞれ表示する
  Then 要約の1行と JSON の両方に "listed" と false が出て、"format_version" は 1 である

@id=EX-919 @about=REQ-473 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A11,docs/decision/brainstorm/2026-09-30-allowlist.md#A10
Scenario: 書かなければ host と出る
  Given どの段も "mounts.mode" を書いていない
  When 要約の計画を表示する
  Then マウントのモードの行は "host" を示す

@id=EX-942 @about=REQ-468 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A42
Scenario: 見せていない場所の秘密の置き場は現れない
  Given 設定ディレクトリに "secrets/" があり、マウントのモードが "listed" で設定ディレクトリを書いていない
  When 隔離の中で設定ディレクトリを調べる
  Then 存在せず、計画は秘密の置き場の "hide" を理由付きで飛ばしたと示す

@id=EX-943 @about=REQ-468 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A43
Scenario: 見えていない本物は包まない
  Given ホストの PATH の先頭の見せていないディレクトリと "/usr/bin" の両方に git があり、git の規則があり、マウントのモードが "listed"
  When 隔離の中で git を起動する
  Then 見張り役は "/usr/bin/git" を本物として起動する

@id=EX-944 @about=REQ-483 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A44
Scenario: 見せていない作業ディレクトリでは起動しない
  Given マウントのモードが "listed" で、作業ディレクトリをどの項目にも書いていない
  When コマンドを起動する
  Then 種類 "path" の診断で終わり、コマンドを実行しない

@id=EX-945 @about=REQ-483 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A44
Scenario: 見せた作業ディレクトリでは起動する
  Given マウントのモードが "listed" で、"${workspace}" を "rw" に書いている
  When ワークスペースでコマンドを起動する
  Then コマンドを実行する

@id=EX-946 @about=REQ-467 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A50
Scenario: host で土台を外すと読み込めない
  Given どの段も "mounts.mode" を書かず、"mounts.system" を false にしている
  When コマンドを起動する
  Then ポリシー読み込み失敗で終わる

@id=EX-949 @about=REQ-468 @source=docs/decision/brainstorm/2026-09-30-allowlist.md#A52
Scenario: リンクで書いた設定をいつものパスで読める
  Given ホームの "a" が "b" を指すリンクで、マウントのモードが "listed" で "~/a" を "ro" に書いている
  When 隔離の中で "~/a" の下のファイルを読む
  Then 読めて、"~/a" は "b" を指すリンクで、そのリンクを書き換えられない
```
