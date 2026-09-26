# コマンドのガードレールの見張り役

ガードレールの規則を隔離の中で当てる見張り役の置き方、見分け方、禁止と通過の振る舞い、計画表示、案内を定義する。規則の形と照合は core-command-guard-rules.md に従う。

## Requirements

### REQ-446: 見張り役を置く

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-command-policy.md#A3, docs/decision/brainstorm/2026-09-25-command-policy.md#A26, docs/decision/brainstorm/2026-09-25-command-policy.md#A28, docs/decision/brainstorm/2026-09-25-command-policy.md#A29, docs/decision/brainstorm/2026-09-25-command-policy.md#A20, docs/decision/brainstorm/2026-09-25-command-policy.md#A35, docs/decision/brainstorm/2026-09-25-command-policy.md#D1, docs/decision/brainstorm/2026-09-25-command-policy.md#A8, docs/decision/brainstorm/2026-09-25-command-policy.md#A38, docs/decision/brainstorm/2026-09-25-command-policy.md#A41, docs/decision/brainstorm/2026-09-25-command-policy.md#A43, docs/decision/brainstorm/2026-09-25-command-policy.md#A44
- verification: unit

合成したポリシーに規則のあるプログラムごとに、kakoiは隔離へ渡すPATH（path-prepend を足した後で、見張り役の場所を除く）でそのプログラムの本物を探し（本物は、隔離の中でその名前で起動されるもので、PATHの項目の順に見て、その名前の場所もリンクを解決した実体もマウントの "hide" で隠されず（自動で生成された "hide" を含め、それを含む項目のうちマウント順で最後に効くものが "hide" でない）、リンクを解決した先が実行できる通常ファイルである最初の同じ名前とする。ディレクトリ、行き先の無いリンク、実行できない通常ファイル、場所か実体が隠される名前は飛ばして先を探す）、kakoi専用のtmpfsの中の見張り役の場所に同じ名前の見張り役を置き、その場所をPATHの最も先頭に足す。規則が "guard-absolute-path" を真にしているときは、本物のリンクを解決した実体のパスにも見張り役を重ね、本物をそのtmpfsの中に置き直す。見張り役はkakoi自身の実行ファイルで、見張り役、規則、置き直した本物は、見張り役の場所を通しては隔離の中から書き換えられない（kakoi自身の実行ファイルが書ける項目の下にあるときは、その項目を通して見張り役の中身が変わる）。見張り役を置くのにkakoi自身の実行ファイルの場所が分からないときは、種類bwrapの診断を出して125で終わる。これらは利用者のマウントより後に重ね、host、none、filteredのすべてで同じにする。kakoiに直接渡したコマンドも、隔離の中で起動される以上この見張り役の置き方に従う。

### REQ-453: 見張り役としての起動を見分ける

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-command-policy.md#A27
- verification: unit

kakoiは、起動された実行ファイルの場所が、自分が置いた見張り役の場所とプログラムの対応に載っていれば見張り役として動く。この判定は入れ子の判定と引数の解析より前に行い、見張り役として動くときは入れ子の警告を出さない。同じ本物を複数のプログラムの名前が指すときは、それらすべての規則を当てる。対応に載っていない場所から起動されたkakoiは、見張り役として動かない。

### REQ-447: 禁止に当たったとき

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-25-command-policy.md#A10, docs/decision/brainstorm/2026-09-25-command-policy.md#A12, docs/decision/brainstorm/2026-09-25-command-policy.md#A35, docs/decision/brainstorm/2026-09-25-command-policy.md#A37, docs/decision/brainstorm/2026-09-25-command-policy.md#A39
- verification: unit

見張り役は、受け取った引数と環境が規則で禁止になるとき、本物を起動せずに、標準エラーに "kakoi: guard: <プログラム名> <当たった語>: <reason>" の1行を出して終了コード126で終わり、標準出力には何も出さない。<プログラム名>は当たった規則の "program" とする。当たった語は引数の側の語で、先頭一致なら当たった語の並び、フラグならそのフラグ、オプションの値ならオプションと値、環境変数ならその名前とし、REQ-289のとおり制御文字を見える表記に逃がす。隔離の外への通知は出さない。

### REQ-448: 禁止に当たらないとき

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-25-command-policy.md#A3, docs/decision/brainstorm/2026-09-25-command-policy.md#A26, docs/decision/brainstorm/2026-09-25-command-policy.md#A40
- verification: unit

見張り役は、受け取った引数と環境がどの規則でも禁止にならないとき、本物を、受け取ったargv[0]を含む同じ引数、同じ環境、同じ作業場所で、自分をexecで置き換えて起動する。その後の終了コード、標準出力、標準エラーは本物のものである。本物のexecに失敗したときは、種類command not executableの診断を出して126で終わる。

### REQ-449: 見張り役を置かないプログラム

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-command-policy.md#A13, docs/decision/brainstorm/2026-09-25-command-policy.md#A19, docs/decision/brainstorm/2026-09-25-command-policy.md#A28, docs/decision/brainstorm/2026-09-25-command-policy.md#A32, docs/decision/brainstorm/2026-09-25-command-policy.md#A35, docs/decision/brainstorm/2026-09-25-command-policy.md#A37, docs/decision/brainstorm/2026-09-25-command-policy.md#A38, docs/decision/brainstorm/2026-09-25-command-policy.md#A44
- verification: unit

規則のあるプログラムについて、本物が隔離へ渡すPATHに見つからないとき、本物がkakoi自身の実行ファイルであるとき、隔離へ渡す環境にPATHが無いときは、そのプログラムに見張り役を置かず、理由を付けて飛ばす。規則の形と例の検証は行う。本物を探すことと隠されるかの判定は、マウントの解決の後に行う。

### REQ-450: 計画表示

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-command-policy.md#A15, docs/decision/brainstorm/2026-09-25-command-policy.md#A35, docs/decision/brainstorm/2026-09-25-command-policy.md#A37, docs/decision/brainstorm/2026-09-25-command-policy.md#A43
- verification: unit

"--print-plan" の要約と全量は、見張り役を置いたプログラムごとに見張り役の場所と置き直した本物の場所（置き直したときだけ）を、飛ばしたプログラムごとに理由を示す。全量はさらに合成した規則とそれぞれの出所を示す。JSONは "guards"（置いたプログラムごとに "program"、見張り役の場所、置き直した本物の場所、当てる規則の出所）と "skipped_guards"（"program" と理由）をキーの追加として載せ、"format_version" は1のままとする。計画の command のパスは、見張り役を通るときは見張り役のパスとする。

### REQ-451: 同梱プロファイルの見本

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-command-policy.md#A14, docs/decision/brainstorm/2026-09-25-command-policy.md#A4, docs/decision/brainstorm/2026-09-25-command-policy.md#A24, docs/decision/brainstorm/2026-09-25-command-policy.md#A31, docs/decision/brainstorm/2026-09-25-command-policy.md#A16, docs/decision/brainstorm/2026-09-25-command-policy.md#A37
- verification: review
- how_to_verify: 同梱プロファイル（examples/profile）を読み、有効な "[[commands.guard]]" が無く、コメントにした git の見本があり、その見本が push の禁止、git のグローバルオプションの宣言、"-c" と "--config-env" で別名を作る値の禁止、理由、当たるべき例と当たってはならない例を持ち、"GIT_CONFIG_*" の環境変数を禁じていないことを確かめる。見本のコメントを外した写しで kakoi が起動し、例の検証を通ることを実行して確かめる。

同梱プロファイルには有効な規則を入れず、git の見本をコメントとして入れる。

### REQ-452: ガードレールの案内

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-command-policy.md#A1, docs/decision/brainstorm/2026-09-25-command-policy.md#A6, docs/decision/brainstorm/2026-09-25-command-policy.md#A7, docs/decision/brainstorm/2026-09-25-command-policy.md#A25, docs/decision/brainstorm/2026-09-25-command-policy.md#A31, docs/decision/brainstorm/2026-09-25-command-policy.md#A34, docs/decision/brainstorm/2026-09-25-command-policy.md#A35, docs/decision/brainstorm/2026-09-25-command-policy.md#A8, docs/decision/brainstorm/2026-09-25-command-policy.md#A26, docs/decision/brainstorm/2026-09-25-command-policy.md#A37, docs/decision/brainstorm/2026-09-25-command-policy.md#A41, docs/decision/brainstorm/2026-09-25-command-policy.md#A42
- verification: review
- how_to_verify: "docs/policy.md" と "docs/security.md" を読み、次が書かれていることを確かめる。ガードレールが境界ではなく悪意があれば迂回できる事故防止の柵であること。見張れないもの（見張り役を置いていない場所やハードリンクからの起動、置き直した本物を探しての直接起動、プログラムを起動せずに同じ操作をするライブラリ、設定ファイルや環境変数で作った別名、組み込みのコマンド、長いオプションの省略形、語の並びの途中のオプション、kakoi自身の実行ファイルが書ける項目の下にあるときにその項目を通して変わる見張り役の中身、"guard-absolute-path" で置き直す本物を計画から起動までの間に書ける別の隔離が差し替えたときに置き直し先に見えうる隠したファイル（既知の隙間15と同じ種類））。"guard-absolute-path" を有効にすると自分の場所から資源を探すプログラムが壊れうること。"git.instead-of" と "GIT_CONFIG_*" の禁止を一緒に使えないこと。本当に止めるには権限の狭いトークンや filtered の通信の許可を使うこと。プログラムそのものを使わせないには "hide" で隠し、守るべき資源（たとえば docker のソケット）があればそれを隠すこと。

公開文書は、ガードレールが境界ではないこと、見張れないもの、"guard-absolute-path" と "git.instead-of" の注意、確実に止める手段、プログラムや資源を隠す "hide" の使い方を示す。

### REQ-454: 責務の分け方

- kind: invariant
- source: docs/decision/brainstorm/2026-09-25-command-policy.md#A36
- verification: review
- how_to_verify: crates/kakoi-core と src を読み、規則の型、読み込み、段の合成、語の照合、例の検証、見張り役を置く計画が crates/kakoi-core にあり、見張り役として起動されたときの処理（場所の対応を読む、引数と環境を受け取る、禁止の1行を出す、本物を exec する）が src にあることを確かめる。例の検証と見張り役が同じ照合の関数を使い、見張り役のための新しいクレートが無いことを確かめる。

規則と照合と見張り役の計画は kakoi-core に置き、見張り役として起動されたときの処理は kakoi の実行ファイルに置く。例の検証と見張り役は同じ照合を使う。

## Examples

```gherkin
@id=EX-862 @about=REQ-446 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A3,docs/decision/brainstorm/2026-09-25-command-policy.md#A10,docs/decision/brainstorm/2026-09-25-command-policy.md#A12
Scenario: PATHで探す起動が見張り役を通る
  Given "git" の "push" を禁じる規則がある
  When 隔離の中で "sh -c 'git push'" を実行する
  Then 見張り役が禁止し、終了コードは126である

@id=EX-863 @about=REQ-446 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A26,docs/decision/brainstorm/2026-09-25-command-policy.md#A10,docs/decision/brainstorm/2026-09-25-command-policy.md#A12
Scenario: guard-absolute-path を有効にすると絶対パスの起動も見張る
  Given "git" の "push" を禁じ "guard-absolute-path" が真の規則があり、"git" の本物が "/usr/bin/git" である
  When 隔離の中で "/usr/bin/git push" を実行する
  Then 見張り役が禁止し、終了コードは126である

@id=EX-877 @about=REQ-446 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A26
Scenario: 既定では本物の場所に重ねない
  Given "git" の "push" を禁じ "guard-absolute-path" を書かない規則があり、"git" の本物が "/usr/bin/git" である
  When 隔離の中で "/usr/bin/git push" を実行する
  Then 見張り役は禁止しない

@id=EX-878 @about=REQ-446 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A28,docs/decision/brainstorm/2026-09-25-command-policy.md#A10,docs/decision/brainstorm/2026-09-25-command-policy.md#A12,docs/decision/brainstorm/2026-09-25-command-policy.md#A3
Scenario: path-prepend の中の本物を見張る
  Given path-prepend の "/opt/tools/bin" に "git" があり、"git" の "push" を禁じる規則がある
  When "kakoi -- git push" を実行する
  Then 見張り役が禁止し、終了コードは126である

@id=EX-864 @about=REQ-446 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A20,docs/decision/brainstorm/2026-09-25-command-policy.md#A10,docs/decision/brainstorm/2026-09-25-command-policy.md#A12
Scenario: kakoiに直接渡したコマンドも見張る
  Given "git" の "push" を禁じる規則がある
  When "kakoi -- git push" を実行する
  Then 終了コードは126で、本物の git は起動されない

@id=EX-865 @about=REQ-446 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A29,docs/decision/brainstorm/2026-09-25-command-policy.md#D1
Scenario: 隔離の中から見張り役を書き換えられない
  Given "git" の規則があり隔離の中で見張り役の場所を知っている
  When 隔離の中から見張り役と規則へ書き込もうとする
  Then どちらも書き込めない

@id=EX-879 @about=REQ-453 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A27
Scenario: 見張り役は入れ子のkakoiとして振る舞わない
  Given "git" の "push" を禁じる規則がある
  When 隔離の中で "git --version" を実行する
  Then 標準出力は本物の git の版で、標準エラーに入れ子の警告を出さない

@id=EX-880 @about=REQ-453 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A27
Scenario: 対応に無い場所のkakoiは見張り役にならない
  Given "git" の規則があり、隔離の中に kakoi の実行ファイルの写しが別の名前 "git" で置いてある
  When その写しを実行する
  Then 見張り役として動かない

@id=EX-866 @about=REQ-447 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A10,docs/decision/brainstorm/2026-09-25-command-policy.md#A12,docs/decision/brainstorm/2026-09-25-command-policy.md#A37
Scenario: 禁止に当たると理由を出して126で終わる
  Given "git" の "push" を禁じ "reason" が "push は人が行う" の規則がある
  When 隔離の中で "git push origin main" を実行する
  Then 標準エラーは "kakoi: guard: git push: push は人が行う" の1行で、標準出力は空、終了コードは126である

@id=EX-881 @about=REQ-447 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A35,docs/decision/brainstorm/2026-09-25-command-policy.md#A10
Scenario: フラグに当たったときはそのフラグを示す
  Given "git" の "deny-flags" が ["--force"] で "reason" が "強制は人が行う" の規則がある
  When 隔離の中で "git push --force" を実行する
  Then 標準エラーは "kakoi: guard: git --force: 強制は人が行う" の1行である

@id=EX-867 @about=REQ-448 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A3
Scenario: 当たらない起動は本物の結果を返す
  Given "git" の "push" を禁じる規則がある
  When 隔離の中で "git --version" を実行する
  Then 標準出力は本物の "git --version" の出力で、終了コードは0である

@id=EX-882 @about=REQ-448 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A26
Scenario: argv[0]と作業場所をそのまま渡す
  Given "sh" の "deny" が [["never-used"]] の規則がある
  When 隔離の中の作業場所 "/w/sub" で "sh -c 'echo $0; pwd'" を実行する
  Then 標準出力は "sh" と "/w/sub" である

@id=EX-868 @about=REQ-449 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A13,docs/decision/brainstorm/2026-09-25-command-policy.md#A15
Scenario: PATHに無いプログラムの規則は飛ばす
  Given 隔離へ渡すPATHに無い "nosuchtool" の規則がある
  When 起動する
  Then 起動は続き、計画表示はそのプログラムを理由付きで飛ばしたと示す

@id=EX-869 @about=REQ-449 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A19,docs/decision/brainstorm/2026-09-25-command-policy.md#A15,docs/decision/brainstorm/2026-09-25-command-policy.md#A35
Scenario: 隠したプログラムには見張り役を置かない
  Given "git" の規則があり、"git" の本物を "hide" で隠している
  When "--print-plan=json" を実行する
  Then "skipped_guards" に "git" が理由付きであり、"guards" に "git" は無い

@id=EX-883 @about=REQ-449 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A28,docs/decision/brainstorm/2026-09-25-command-policy.md#A35
Scenario: PATHの無い環境では見張り役を置かない
  Given "env.mode" が "clear" で "PATH" を渡さず、"git" の規則がある
  When "--print-plan=json" を実行する
  Then "skipped_guards" に "git" が理由付きである

@id=EX-870 @about=REQ-450 @source=docs/decision/brainstorm/2026-09-25-command-policy.md#A15,docs/decision/brainstorm/2026-09-25-command-policy.md#A35
Scenario: 計画表示が見張り役の場所を示す
  Given "git" の規則がある
  When "--print-plan=json" を実行する
  Then "guards" に "git" の見張り役の場所と規則の出所があり、"format_version" は1である
```
