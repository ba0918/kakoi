# コマンドのガードレールの規則

隔離の中で起動されるプログラムの使い方を止めるガードレールの規則の形、照合、合成を定義する。ガードレールは悪意があれば迂回できる事故防止の柵で、境界ではない。

## Requirements

### REQ-438: 規則の形

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-command-policy.md#A16, docs/decision/brainstorm/2026-09-25-command-policy.md#A33, docs/decision/brainstorm/2026-09-25-command-policy.md#A35, docs/decision/brainstorm/2026-09-25-command-policy.md#A2, docs/decision/brainstorm/2026-09-25-command-policy.md#A24, docs/decision/brainstorm/2026-09-25-command-policy.md#A26, docs/decision/brainstorm/2026-09-25-command-policy.md#A37
- verification: unit

ガードレールの規則はポリシーの "[[commands.guard]]" に書く。各規則は "program"（PATHで探す名前で、空でなく "/" を含まず "kakoi" でないもの）と "reason"（空でない文字列）を必ず持ち、"options-with-value"、"for"、"deny"、"deny-flags"、"deny-option-values"、"deny-env"、"guard-absolute-path"、"examples.deny"、"examples.allow" を任意で持つ。"deny"、"deny-flags"、"deny-option-values"、"deny-env" のどれも空でない形で持たない規則、空の一覧や空の語の並びを持つ規則、条件に合わない "program" を持つ規則、知らないキーを持つ規則は形の誤りとする。

### REQ-439: 語の照合

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-command-policy.md#A17, docs/decision/brainstorm/2026-09-25-command-policy.md#A35
- verification: unit

"deny" と "for" の語、"deny-option-values" の値は語全体と照合する。"/" で始まり "/" で終わる2文字以上の文字列は、囲まれた部分をRustのregexクレートの構文の正規表現として語全体に当て、それ以外の文字列は語との完全一致とする。"options-with-value"、"deny-flags"、"deny-option-values" のオプション名は常に完全一致とする。UTF-8として読めない語はどの照合にも当たらない。

### REQ-440: 使い方の先頭一致

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-command-policy.md#A4, docs/decision/brainstorm/2026-09-25-command-policy.md#A21, docs/decision/brainstorm/2026-09-25-command-policy.md#A34, docs/decision/brainstorm/2026-09-25-command-policy.md#A35, docs/decision/brainstorm/2026-09-25-command-policy.md#A33, docs/decision/brainstorm/2026-09-25-command-policy.md#A37
- verification: unit

"deny" と "for" の各項目は語の並びで、位置ごとに1つの語か語の代替の一覧を書く。照合の前に、プログラム名の直後から "-" で始まる語を読み飛ばす。"options-with-value" に書いた名前の語は次の語も値として読み飛ばし、"=" を含む語はその1語だけを読み飛ばし、それ以外の "-" で始まる語は値を取らないものとして読み飛ばす。"--" の語が来たら、それを読み飛ばして読み飛ばしを終える。語の並びの途中にある "-" で始まる語は読み飛ばさない。残った語の並びの先頭が "deny" の項目の語の並びにすべて当たれば禁止とする。項目より後ろの語は問わない。

### REQ-441: 位置を問わないフラグとオプションの値

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-command-policy.md#A18, docs/decision/brainstorm/2026-09-25-command-policy.md#A24, docs/decision/brainstorm/2026-09-25-command-policy.md#A33, docs/decision/brainstorm/2026-09-25-command-policy.md#A35, docs/decision/brainstorm/2026-09-25-command-policy.md#A37
- verification: unit

"deny-flags" の各フラグは、プログラム名より後で最初の "--" の語より前の語に照合する。語が "=" を含むときは最初の "=" より前の部分を照合する。"-" に続けて1文字のフラグは、"-" で始まり "--" で始まらない語に含まれる文字にも当てる。"deny-option-values" はオプションの名前ごとに禁じる値の一覧を持ち、最初の "--" の語より前で、その名前の語の次の語、またはその名前に "=" を付けた語の最初の "=" より後ろの部分が、一覧のどれかに当たれば禁止とする。規則が "for" を持つときは、REQ-440の読み飛ばしの後の先頭の語の並びが "for" のどれかの項目に当たる起動にだけ、この2つを当てる。

### REQ-442: 環境変数

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-command-policy.md#A16, docs/decision/brainstorm/2026-09-25-command-policy.md#A33, docs/decision/brainstorm/2026-09-25-command-policy.md#A35
- verification: unit

"deny-env" の各項目は環境変数の名前の型で、ワイルドカードの意味はREQ-155に従う。見張り役が起動されたときの環境に、どれかの型に当たる名前の変数があれば禁止とする。規則が "for" を持つときは、REQ-441と同じく "for" に当たる起動にだけ当てる。

### REQ-443: 照合の順と合成

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-command-policy.md#A9, docs/decision/brainstorm/2026-09-25-command-policy.md#A35, docs/decision/brainstorm/2026-09-25-command-policy.md#A4
- verification: unit

"commands.guard" の規則は段をまたいで連結し、上の段で下の段の規則を消すことはできない。見張り役は、その起動に当てる規則ごとに "deny-env"、"deny"、"deny-flags"、"deny-option-values" の順で照合し、最初に当たったもので禁止とする。どの規則にも当たらなければ禁止にしない。

### REQ-444: 例の検証

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-command-policy.md#A4, docs/decision/brainstorm/2026-09-25-command-policy.md#A22, docs/decision/brainstorm/2026-09-25-command-policy.md#A23, docs/decision/brainstorm/2026-09-25-command-policy.md#A30, docs/decision/brainstorm/2026-09-25-command-policy.md#A32
- verification: unit

"examples.deny" と "examples.allow" の各例は、シェルと同じ引用の規則で語に分ける文字列とする。先頭から続く "NAME=value" の形の語は環境として読み、残りの語をコマンドとする。ポリシーを読み込んで合成するとき、各例をその環境だけで照合し、"examples.deny" の例はその規則で禁止になり、"examples.allow" の例はその規則で禁止にならないことを確かめる。コマンドの先頭の語が規則の "program" と異なるとき、期待と異なるとき、正規表現が壊れているとき、規則が形の誤りのときは、種類 "policy" の診断で終わる。説明には規則の "program" と、期待と異なった例を含める。"--print-plan" も同じ診断で終わる。

## Examples

```gherkin
@id=EX-846 @about=REQ-438 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A16,docs/decision/brainstorm/2026-09-25-command-policy.md#A23
Scenario: 禁止の書き方を持たない規則を拒否する
  Given "[[commands.guard]]" に "program" と "reason" だけを書いた規則がある
  When ポリシーを読み込む
  Then 種類 "policy" の診断で終わる

@id=EX-847 @about=REQ-438 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A16,docs/decision/brainstorm/2026-09-25-command-policy.md#A35,docs/decision/brainstorm/2026-09-25-command-policy.md#A23,docs/decision/brainstorm/2026-09-25-command-policy.md#A37
Scenario: 条件に合わない program と知らないキーを拒否する
  Given "program" が "/usr/bin/git" の規則、"program" が "kakoi" の規則、知らないキー "allow" を持つ規則のどれか1つがある
  When ポリシーを読み込む
  Then 種類 "policy" の診断で終わる

@id=EX-871 @about=REQ-438 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A16
Scenario: 形の正しい規則を読み込める
  Given "program" が "git"、"deny" が [["push"]]、"reason" が "push は人が行う" の規則がある
  When ポリシーを読み込む
  Then 診断を出さない

@id=EX-848 @about=REQ-439 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A17
Scenario: 普通の文字列は語の一部に当たらない
  Given "program" が "git" で "deny" が [["push"]] の規則がある
  When "git pushx" を照合する
  Then 禁止にならない

@id=EX-849 @about=REQ-439 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A17
Scenario: 正規表現は語全体に当てる
  Given "program" が "git" で "deny" が [["/pu.h/"]] の規則がある
  When "git push" と "git pushx" を照合する
  Then "git push" は禁止になり "git pushx" は禁止にならない

@id=EX-850 @about=REQ-440 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A4,docs/decision/brainstorm/2026-09-25-command-policy.md#A21
Scenario: 値を取るグローバルオプションを読み飛ばして当てる
  Given "program" が "git"、"options-with-value" が ["-C"]、"deny" が [["push"]] の規則がある
  When "git --no-pager -C push push origin" を照合する
  Then 禁止になる

@id=EX-851 @about=REQ-440 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A4,docs/decision/brainstorm/2026-09-25-command-policy.md#A21
Scenario: 宣言していない値は語として残る
  Given "program" が "git"、"options-with-value" が空、"deny" が [["push"]] の規則がある
  When "git -C dir push" を照合する
  Then 宣言していない "-C" は値を取らないものとして読み飛ばされ、残る "dir push" は "push" で始まらないので禁止にならない

@id=EX-852 @about=REQ-440 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A4
Scenario: 先頭一致で引数の中の語には当たらない
  Given "program" が "git" で "deny" が [["push"]] の規則がある
  When "git commit -m push" を照合する
  Then 禁止にならない

@id=EX-853 @about=REQ-440 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A4,docs/decision/brainstorm/2026-09-25-command-policy.md#A34
Scenario: 位置ごとの代替に当て、途中のオプションは読み飛ばさない
  Given "program" が "git" で "deny" が [["remote", ["add", "set-url"]]] の規則がある
  When "git remote set-url origin x"、"git remote -v"、"git remote -v add origin x" を照合する
  Then 最初だけが禁止になる

@id=EX-872 @about=REQ-440 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A21,docs/decision/brainstorm/2026-09-25-command-policy.md#A37
Scenario: "--" で読み飛ばしを終える
  Given "program" が "git" で "deny" が [["push"]] の規則がある
  When "git -- push" と "git -- -x push" を照合する
  Then "git -- push" は禁止になり "git -- -x push" は禁止にならない

@id=EX-854 @about=REQ-441 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A18
Scenario: まとめ書きと等号付きのフラグに当てる
  Given "program" が "git" で "deny-flags" が ["-f", "--force"] の規則がある
  When "git push -fq" と "git push --force=yes" を照合する
  Then どちらも禁止になる

@id=EX-855 @about=REQ-441 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A18
Scenario: "--" より後のフラグには当てない
  Given "program" が "git" で "deny-flags" が ["--force"] の規則がある
  When "git log -- --force" を照合する
  Then 禁止にならない

@id=EX-856 @about=REQ-441 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A24,docs/decision/brainstorm/2026-09-25-command-policy.md#A35
Scenario: 別名を作る値だけを止める
  Given "program" が "git"、"options-with-value" が ["-c", "--config-env"]、"deny-option-values" が "-c" と "--config-env" に ["/alias[.].*/"] の規則がある
  When "git -c alias.p=push p"、"git --config-env=alias.p=ENV p"、"git -c color.ui=false log" を照合する
  Then 最初の2つは禁止になり、最後は禁止にならない

@id=EX-873 @about=REQ-441 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A33
Scenario: for に当たる起動にだけフラグを当てる
  Given "program" が "git"、"for" が [["push"]]、"deny-flags" が ["-f"] の規則がある
  When "git push -f" と "git checkout -f" を照合する
  Then "git push -f" は禁止になり "git checkout -f" は禁止にならない

@id=EX-857 @about=REQ-442 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A16
Scenario: 名前の型に当たる環境変数があれば止める
  Given "program" が "git" で "deny-env" が ["GIT_CONFIG_*"] の規則がある
  When 環境に "GIT_CONFIG_COUNT" がある状態で "git status" を照合する
  Then 禁止になる

@id=EX-858 @about=REQ-442 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A16
Scenario: 当たる環境変数が無ければ止めない
  Given "program" が "git" で "deny-env" が ["GIT_CONFIG_*"] の規則がある
  When 環境に "GIT_DIR" だけがある状態で "git status" を照合する
  Then 禁止にならない

@id=EX-861 @about=REQ-443 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A9
Scenario: 上の段の規則は下の段の規則を消さない
  Given 下の段に "git" の "push" を禁じる規則、上の段に "git" の "fetch" を禁じる規則がある
  When 合成した規則で "git push" と "git fetch" を照合する
  Then どちらも禁止になる

@id=EX-874 @about=REQ-443 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A35,docs/decision/brainstorm/2026-09-25-command-policy.md#A4
Scenario: どの規則にも当たらなければ止めない
  Given "git" の "push" を禁じる規則だけがある
  When "git status" を照合する
  Then 禁止にならない

@id=EX-859 @about=REQ-444 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A23,docs/decision/brainstorm/2026-09-25-command-policy.md#A21,docs/decision/brainstorm/2026-09-25-command-policy.md#A4
Scenario: 期待と違う例で起動しない
  Given "deny" が [["push"]] で "options-with-value" が空、"examples.deny" に "git -C dir push" がある規則がある
  When ポリシーを読み込む
  Then 種類 "policy" の診断で終わり、説明に "git -C dir push" を含む

@id=EX-860 @about=REQ-444 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A22,docs/decision/brainstorm/2026-09-25-command-policy.md#A23,docs/decision/brainstorm/2026-09-25-command-policy.md#A4
Scenario: 例どおりに当たる規則は読み込める
  Given "deny" が [["push"]] で "examples.deny" に "git push" と "examples.allow" に "git commit -m 'push fix'" がある規則がある
  When ポリシーを読み込む
  Then 診断を出さない

@id=EX-875 @about=REQ-444 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A30,docs/decision/brainstorm/2026-09-25-command-policy.md#A16
Scenario: 例の先頭の環境で deny-env を確かめる
  Given "deny-env" が ["GIT_CONFIG_*"] で "examples.deny" に "GIT_CONFIG_COUNT=1 git status" と "examples.allow" に "git status" がある規則がある
  When ホストの環境に "GIT_CONFIG_COUNT" がある状態でポリシーを読み込む
  Then 診断を出さない

@id=EX-876 @about=REQ-444 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A23,docs/decision/brainstorm/2026-09-25-command-policy.md#A35
Scenario: 壊れた正規表現で起動しない
  Given "deny" が [["/pu(sh/"]] の規則がある
  When "--print-plan" で起動する
  Then 種類 "policy" の診断で終わる
```
