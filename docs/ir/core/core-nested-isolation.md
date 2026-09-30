# 入れ子の中の隔離

入れ子の判定、呼ぶ側が頼んだときに入れ子の中で隔離を作り直すこと、それを支える外の隔離の備え（tun、共有ファイルの置き場、resolver の待ち受け）と、その公開文書を扱う。入れ子で隔離を作らずに実行する既定の動きは core-process.md にある。

## Requirements

### REQ-455: 入れ子の判定
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-29-nested-isolation.md#A23, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A24, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A30
- verification: unit

kakoiは隔離の中の "/dev/kakoi-isolated" に、読み取り専用のマウントで入れ子の印のファイルを置く。kakoiは起動時にこのファイルが存在するときだけ入れ子と判定し、環境変数 "KAKOI" の有無と値を判定に使わない。隔離の中の環境には今までどおり "KAKOI=1" を置く。

### REQ-456: 入れ子の中で隔離を作る実行
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-29-nested-isolation.md#A2, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A3, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A6, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A7, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A12, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A29, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A19
- verification: unit

"--nested=isolate" を付けた実行は、入れ子の判定の結果によらず、入れ子でない起動と同じ手順でポリシーの読み込み、検査、隔離の作成を行ってからコマンドを実行する。違いはファイルの置き方（REQ-460）だけである。外の隔離の制限は中でもそのまま効き、中の隔離は外より広がらない。入れ子の警告を出さない。隔離を作れないとき（外の隔離が秘密ファイルを空にしていて秘密を読めないとき、"/dev/net/tun" が無く filtered を作れないときを含む）は、コマンドを実行せず、その原因の診断で終わる。隔離を作らずにコマンドを実行する動きに移らない。

### REQ-457: 入れ子の中で隔離を作る計画
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-29-nested-isolation.md#A8, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A9, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A26, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A30
- verification: unit

"--nested=isolate" を付けた入れ子の計画の表示は、入れ子の表示を付け、その計画が使われることと、外の隔離の制限と重なって計画より狭くなりうることを示す。JSONの "nested" は入れ子の判定の結果を表す。JSONの "applied" は計画が使われるかを表し、入れ子でない起動と "--nested=isolate" の入れ子の起動では true、"--nested=exec" の入れ子の起動では false とする。"format_version" は 1 のままとする。

### REQ-458: 入れ子の filtered のための tun
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-29-nested-isolation.md#A14, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A30, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A31, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A32, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A34, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A44
- verification: unit

ポリシーの "network.allow-nested-filtered" は真偽値で、既定は false とし、段をまたぐと上の段の値が勝つ。true のとき、kakoiはネットワークのモードによらず隔離の中にホストの "/dev/net/tun" を見せ、計画の要約にそのことを1行示す。false のときは見せない。true でホストに "/dev/net/tun" が無いときは、コマンドを実行せず種類 bwrap の診断で 125 とする。この確かめは bwrap の所在の段階で行い、計画表示でも行う。

### REQ-459: 入れ子の filtered の resolver
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-29-nested-isolation.md#A17
- verification: unit

filtered の隔離の中の resolver は "127.0.0.53" と "127.0.0.54" の両方のポート 53 で待ち受ける。隔離の中の "/etc/resolv.conf" の中身は "nameserver 127.0.0.53" の1行のままとする。

### REQ-460: 実在のファイルの共有ファイルの置き場
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-29-nested-isolation.md#A16, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A42, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A19, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A20, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A33, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A35
- verification: unit

入れ子でない起動は、ファイルの "hide" に使う空のファイルと filtered の "/etc/resolv.conf" を、共有ファイルの置き場 "$XDG_RUNTIME_DIR/kakoi/" の実在のファイルの読み取り専用の bind で置く。共有ファイルの置き場を隔離の中に見せるかはポリシーに従う。入れ子の起動と、共有ファイルの置き場を使えない起動（"XDG_RUNTIME_DIR" が無い、共有ファイルの置き場を作れない、REQ-461 の確かめを通せない）は、データから作るファイルで置き、警告を出さない。どちらで置いても、隔離の中からは権限 0600 の通常ファイルで、中身は空か "nameserver 127.0.0.53" の1行に見える。共有ファイルの置き場のファイルの権限は 0600、共有ファイルの置き場のディレクトリの権限は 0700 とする。ファイルの "rw-copy" の置き方は変えない。

### REQ-461: 共有ファイルの置き場の確かめ
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-29-nested-isolation.md#A21, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A38, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A43, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A20, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A46
- verification: unit

共有ファイルの置き場を使う起動は、使う前に、置き場のディレクトリが利用者の持ち物でシンボリックリンクでなく権限 0700 であること、置き場のファイルごとに利用者の持ち物でシンボリックリンクでなく権限 0600 の通常ファイルで中身が決まったとおりであることを確かめる。ディレクトリが無ければ権限 0700 で作る。ファイルがどれかに違えば、置き場の中に一時的な名前で書いてから名前を付け替えて作り直す。作り直した後も確かめを通せなければ、その起動は置き場を使えない起動として扱う。"XDG_RUNTIME_DIR" が空か絶対パスでないときも、置き場を使えない起動とする。この確かめと作り直しは、実際の起動でコマンドを解決した後、bwrap を起動する直前に行う。計画表示は置き場に書き込まず、入れ子でなく "XDG_RUNTIME_DIR" が絶対パスなら置き場を使う形で計画を示す。

### REQ-462: 共有ファイルの置き場の保護
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-29-nested-isolation.md#A22, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A36, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A43
- verification: unit

共有ファイルの置き場を使う起動では、置き場のディレクトリを REQ-158 の保護対象のパスに加え、その3つの条件をそのまま当てる。合成後の "rw" または "rw-file" の項目が置き場の中にあるときも、同じ種類 "path" の診断で終わる。どちらの場合も、置き場を使えない起動として続けることはしない。保護対象の診断の順（REQ-295）では、置き場を最後に置く。この検査は計画表示でも行う。

### REQ-465: 入れ子の中の隔離と外の見張り役
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-29-nested-isolation.md#A51, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A53, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A55
- verification: unit

"--nested=isolate" の起動は、起動したときに "/dev/kakoi-guard" が存在すれば、それを隔離の中の同じ場所に読み取り専用でそのまま見せる。存在しなければ何もしない。外の見張り役の規則と、本物の場所に重ねた見張り役は、中の隔離でも外と同じに働く。中の環境に PATH が無いときは、REQ-268 のとおり見張り役の場所を PATH に足さないので、PATH で探す見張り役は働かない。

### REQ-466: 入れ子の中の隔離で重ねられない見張り役
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-29-nested-isolation.md#A52, docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A51, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A53
- verification: unit

"--nested=isolate" の起動が "/dev/kakoi-guard" を引き継ぐときに、中の合成後のポリシーに見張り役の規則が1つでもあれば、コマンドを実行せず種類 policy の診断で 125 とする。

### REQ-463: 入れ子の中の隔離の公開文書
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-29-nested-isolation.md#A12, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A13, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A24, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A28, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A47, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A10, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A23, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A30
- verification: review
- how_to_verify: "docs/cli.md" の Nesting 節、"docs/policy.md" の network 節、"docs/security.md" の既知の隙間と未対応の一覧を読み、本文の各事項が書かれていることと、TBL-160 の行と一致することを確かめる。

"docs/cli.md" の入れ子の説明に、入れ子の印による判定、"--nested" の2つの値と既定、"KAKOI=1" は目安であって境界ではないこと、"--nested=isolate" の入れ子で秘密を読めずに止まる場合の回避（秘密は外の kakoi で渡すか "env.pass" で外から受け継ぐ）を書く。"docs/policy.md" に "network.allow-nested-filtered" を書く。"docs/security.md" の未対応の一覧から入れ子の二重の隔離と環境変数以外での入れ子の判定を外し、既知の隙間を TBL-160 に合わせる。

## Examples

```gherkin
@id=EX-884 @about=REQ-455 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A23,docs/decision/brainstorm/2026-09-29-nested-isolation.md#A1,docs/decision/brainstorm/2026-09-29-nested-isolation.md#A10
Scenario: 環境変数を消しても入れ子と分かる
  Given 隔離の中で環境変数 KAKOI を消している
  When "--nested" を付けずに kakoi でコマンドを起動する
  Then 入れ子の印を見て入れ子と判定し、隔離を作らずに入れ子の警告を出して実行する

@id=EX-885 @about=REQ-455 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A23
Scenario: ホストで KAKOI=1 を立てても隔離される
  Given ホストで環境変数 KAKOI=1 を立てている
  When "--nested" を付けずに kakoi でコマンドを起動する
  Then 入れ子の印が無いので隔離を作ってから実行し、入れ子の警告を出さない

@id=EX-886 @about=REQ-456 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A2,docs/decision/brainstorm/2026-09-29-nested-isolation.md#A7
Scenario: 入れ子の中で頼んだ隔離は外より広がらない
  Given 外の隔離が "rw" にしていない場所を、中のポリシーが "rw" にしている
  When 入れ子の中で "--nested=isolate" を付けてその場所に書く
  Then 隔離の中から書けず、入れ子の警告も出ない

@id=EX-887 @about=REQ-456 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A3
Scenario: 入れ子の中で filtered を作れなければ実行しない
  Given 外の隔離が "/dev/net/tun" を見せていない
  When 入れ子の中で "--nested=isolate" と filtered のポリシーでコマンドを起動する
  Then コマンドを実行せずに診断で終わる

@id=EX-888 @about=REQ-457 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A9,docs/decision/brainstorm/2026-09-29-nested-isolation.md#A8,docs/decision/brainstorm/2026-09-29-nested-isolation.md#A26
Scenario: 計画の applied
  Given 入れ子の中と、入れ子でないホストで起動する
  When 入れ子の中で "--nested=isolate" と "--nested=exec"、ホストで "--nested" を付けずに、それぞれ JSON の計画を表示する
  Then "nested" は入れ子の中の2つだけ true で、"applied" は入れ子の中の "--nested=exec" だけ false になる

@id=EX-889 @about=REQ-458 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A14
Scenario: 外が tun を許すと入れ子で filtered を作れる
  Given 外のポリシーが host で "network.allow-nested-filtered = true" にしている
  When 入れ子の中で "--nested=isolate" と許可先を1つだけ書いた filtered のポリシーで通信する
  Then 許可先には繋がり、許可していない宛先には繋がらない

@id=EX-890 @about=REQ-458 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A14
Scenario: 既定では tun を見せない
  Given ポリシーに "network.allow-nested-filtered" を書いていない
  When 隔離の中で "/dev/net/tun" を探す
  Then 見つからない

@id=EX-891 @about=REQ-459,REQ-460 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A16,docs/decision/brainstorm/2026-09-29-nested-isolation.md#A17
Scenario: 外も中も filtered で動く
  Given 外の起動は共有ファイルの置き場を使えていて、外のポリシーが filtered で "network.allow-nested-filtered = true" にしている
  When 入れ子の中で "--nested=isolate" と、外も許す名前を1つだけ許可した filtered のポリシーで、その名前に通信する
  Then 名前が解決されて繋がり、外が許して中が許していない宛先には繋がらない

@id=EX-892 @about=REQ-460 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A16
Scenario: 外と中で同じファイルを隠せる
  Given 外の起動は共有ファイルの置き場を使えていて、外のポリシーと中のポリシーが同じファイルを "hide" にしている
  When 入れ子の中で "--nested=isolate" を付けてコマンドを起動する
  Then 隔離を作ってコマンドを実行し、そのファイルは空に見える

@id=EX-893 @about=REQ-460 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A20,docs/decision/brainstorm/2026-09-29-nested-isolation.md#A33
Scenario: 共有ファイルの置き場が無くても入れ子でない起動は続く
  Given 環境変数 XDG_RUNTIME_DIR が無い
  When ファイルを "hide" にするポリシーでコマンドを起動する
  Then 警告を出さずに実行し、そのファイルは権限 0600 の空の通常ファイルに見える

@id=EX-894 @about=REQ-461 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A21
Scenario: 書き換えられた共有ファイルの置き場のファイルは作り直す
  Given 共有ファイルの置き場の空のファイルに中身が書き込まれている
  When ファイルを "hide" にするポリシーでコマンドを起動する
  Then 隠したファイルは空に見え、共有ファイルの置き場のファイルも空に戻っている

@id=EX-895 @about=REQ-462 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A22,docs/decision/brainstorm/2026-09-29-nested-isolation.md#A36,docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 共有ファイルの置き場を書き込める項目で覆うと止まる
  Given 入れ子でない起動で、環境変数 XDG_RUNTIME_DIR が "/run/user/1000" で、ポリシーが "/run/user/1000" を "rw" にしている
  When コマンドを起動する
  Then コマンドを実行せず種類 path の診断で 125 になる

@id=EX-898 @about=REQ-460 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A35,docs/decision/brainstorm/2026-09-29-nested-isolation.md#A16,docs/decision/brainstorm/2026-09-29-nested-isolation.md#A19
Scenario: 共有ファイルの置き場で隠したファイルには書けない
  Given 入れ子でない起動で共有ファイルの置き場を使えていて、ポリシーがファイルを "hide" にしている
  When 隔離の中からそのファイルに書く
  Then 書けず、置き場のファイルは空のままである

@id=EX-899 @about=REQ-465 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A51,docs/decision/brainstorm/2026-09-25-command-policy.md#A12
Scenario: 外の見張り役の規則は入れ子の中の隔離でも効く
  Given 外のポリシーが "git push" を禁じる見張り役を置いている
  When 入れ子の中で見張り役の無いポリシーと "--nested=isolate" でシェルを起動し、その中で "git push" を実行する
  Then 見張り役が禁止を伝えて 126 で終わる

@id=EX-900 @about=REQ-465 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A51,docs/decision/brainstorm/2026-09-25-command-policy.md#A26
Scenario: 本物の場所に重ねた見張り役は入れ子の中の隔離でも本物を起動する
  Given 外のポリシーが git の見張り役を "guard-absolute-path = true" で置いている
  When 入れ子の中で見張り役の無いポリシーと "--nested=isolate" でシェルを起動し、その中で "/usr/bin/git --version" を実行する
  Then 本物の git の版が出る

@id=EX-901 @about=REQ-466 @source=docs/decision/brainstorm/2026-09-29-nested-isolation.md#A52,docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 外と中の両方に見張り役があると入れ子の中の隔離を作らない
  Given 外のポリシーが見張り役を置いている
  When 入れ子の中で見張り役のあるポリシーと "--nested=isolate" でコマンドを起動する
  Then コマンドを実行せず種類 policy の診断で 125 になる
```
