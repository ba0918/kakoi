# filteredのpastaの導入

filteredが使うpastaが無い、または古いときの診断と、利用者が新しいpastaを入れるための案内を定義する。

## Requirements

### REQ-429: pastaがPATHに無いときの診断

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-25-pasta-install.md#A6, docs/decision/brainstorm/2026-09-25-pasta-install.md#A5, docs/decision/brainstorm/2026-09-25-pasta-install.md#A7, docs/decision/brainstorm/2026-09-25-pasta-install.md#A22, docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

filteredの起動でホストのPATHにpastaが見つからないとき、kakoiは種類bwrapの診断を出して125で終わる。診断の説明文は導入手順の文書のURL "https://github.com/ba0918/kakoi/blob/main/docs/pasta.md" を含む。pastaとnftがともにPATHに無いときはpastaを報告する。"--print-plan" はpastaの所在を確かめない。

### REQ-430: 必要なオプションを持たないpastaの診断

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-25-pasta-install.md#A1, docs/decision/brainstorm/2026-09-25-pasta-install.md#A4, docs/decision/brainstorm/2026-09-25-pasta-install.md#A5, docs/decision/brainstorm/2026-09-25-pasta-install.md#A10, docs/decision/brainstorm/2026-09-25-pasta-install.md#A18, docs/decision/brainstorm/2026-09-25-pasta-install.md#A19, docs/decision/brainstorm/2026-09-25-pasta-install.md#A20, docs/decision/brainstorm/2026-09-25-pasta-install.md#A22, docs/decision/brainstorm/2026-09-25-pasta-install.md#A7
- verification: unit

filteredの最初の起動で、準備完了の合図より前にpastaのプロセスが終了したとき（起動用のパイプが先に閉じてから終了を確かめた場合を含む）、kakoiは同じpastaを "--help" 付きで実行し、標準出力と標準エラーを合わせた出力に、確かめる名前がそれぞれ載っているかを確かめる。確かめる名前は、filteredの2段のpastaにkakoiが "--" で始まる長い名前で渡すオプションを合わせたもので、pastaの引数を組み立てるときと同じ一覧から取る。名前は、前後が空白、カンマ、行の端のどれかである出現を載っているとみなす。載っていない名前が1つ以上あれば、種類bwrapの診断を出して125で終わる。その説明文は載っていない名前をすべてと、導入手順の文書のURL "https://github.com/ba0918/kakoi/blob/main/docs/pasta.md" を含み、pastaの出力を含まない。

### REQ-431: 古さを確かめない場合

- kind: prohibition
- source: docs/decision/brainstorm/2026-09-25-pasta-install.md#A4, docs/decision/brainstorm/2026-09-25-pasta-install.md#A19, docs/decision/brainstorm/2026-09-25-pasta-install.md#A22
- verification: unit

kakoiは次の場合にpastaを "--help" 付きで実行しない。filteredの起動でpastaが起動に成功したとき。pastaの起動がタイムアウト、PIDの不正、取り消しで終わったとき。通信障害から立ち直るときのpastaの再起動が失敗したとき。"--print-plan" のとき。

### REQ-432: 古さ以外で終了したpastaの診断

- kind: event_driven
- source: docs/decision/brainstorm/2026-09-25-pasta-install.md#A11, docs/decision/brainstorm/2026-09-25-pasta-install.md#A18, docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

REQ-430の "--help" 付きの実行が失敗したとき（実行できなかった、または標準出力と標準エラーを合わせた出力が空だった）、または確かめる名前がすべて載っていたとき、kakoiは種類bwrapの診断に、pastaが終了時に出した理由を付けて125で終わる。この場合、診断は導入手順の文書へ案内しない。"--help" 付きの実行の終了コードでは判定しない。

### REQ-433: pastaをPATHだけから探す

- kind: prohibition
- source: docs/decision/brainstorm/2026-09-25-pasta-install.md#A8
- verification: review
- how_to_verify: crates/kakoi-netのpastaの所在確認を読み、ホストのPATHだけを探していることを確かめる。ポリシーと環境変数のどちらにもpastaの場所を指定する項目が無いことを、"docs/policy.md" と実装の読み込み処理で確かめる。

kakoiはpastaをホストのPATHだけから探し、ポリシーと環境変数でpastaの場所を指定する手段を持たない。

### REQ-434: pastaの導入手順の文書

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-pasta-install.md#A2, docs/decision/brainstorm/2026-09-25-pasta-install.md#A7, docs/decision/brainstorm/2026-09-25-pasta-install.md#A12, docs/decision/brainstorm/2026-09-25-pasta-install.md#A16, docs/decision/brainstorm/2026-09-25-pasta-install.md#A17, docs/decision/brainstorm/2026-09-25-pasta-install.md#A21, docs/decision/brainstorm/2026-09-25-pasta-install.md#A20, docs/decision/brainstorm/2026-09-25-pasta-install.md#A8, docs/decision/brainstorm/2026-09-25-pasta-install.md#A15
- verification: review
- how_to_verify: "docs/pasta.md" を読み、英語で書かれ、次の4つを載せていることを確かめる。導入済みのpastaの "--help" の出力に、kakoiが長い名前で渡すオプションの名前がすべて載っているかで確かめる方法と、その名前の一覧がcrates/kakoi-netのpastaの引数を組み立てる一覧の長い名前と一致すること。オプションが揃う最低の版 "2024_10_30"、推奨する版 "2026_07_16" 以降、確かめた版 "2026_07_28.f8df3f1"。上流のpasstのgitからタグを指定してソースビルドし "~/.local/bin" に置く手順と、それがPATHで古いpastaより前に来る必要があること。Ubuntu 23.10以降では非特権のuser名前空間の制限を外す必要があるという一文と "docs/getting-started.md" の該当節へのリンク。podman-staticから取り出す手順とディストリごとの例や版の一覧が無いことを確かめる。READMEから "docs/pasta.md" へリンクしていることを確かめる。ソースビルドの手順は、公開前に書かれたとおりに実行して、置いたpastaの "--help" に文書が載せた名前がすべて載ることを観測する。

"docs/pasta.md" に英語で、導入済みのpastaの確かめ方（kakoiが長い名前で渡すオプションの名前をすべて載せる）、必要な版、足りない場合のソースビルドの手順、Ubuntu 23.10以降のuser名前空間の制限への案内を載せる。READMEからこの文書へ案内する。

### REQ-435: セットアップスキルのpastaの確認

- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-pasta-install.md#A3, docs/decision/brainstorm/2026-09-25-pasta-install.md#A9, docs/decision/brainstorm/2026-09-25-pasta-install.md#A21
- verification: review
- how_to_verify: "skills/kakoi-setup/SKILL.md" を読み、利用者がfilteredを使うか使いたいときだけpastaを調べること、調べ方が "pasta --help" の出力に "docs/pasta.md" に載った名前がすべて載っているかであること、確かめる名前を文書から取りスキルに写していないこと、足りなければ "docs/pasta.md" を読んで手順を示すこと、手順をスキルに写していないこと、pastaの導入をスキルが実行せず利用者が行うことが書かれていることを確かめる。

セットアップスキルは、利用者がfilteredを使うか使いたいときだけ、導入済みのpastaの "pasta --help" の出力に "docs/pasta.md" に載った名前がすべて載っているかを調べる。足りなければ "docs/pasta.md" を読んで手順を示し、導入は利用者が行う。スキルは手順を写さず、pastaの導入を実行しない。

## Examples

```gherkin
@id=EX-824 @about=REQ-429 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A6,docs/decision/brainstorm/2026-09-25-pasta-install.md#A5,docs/decision/brainstorm/2026-09-25-pasta-install.md#A7,docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: PATHにpastaが無いときに導入手順へ案内する
  Given ホストのPATHにpastaが無い
  When filteredで起動する
  Then 種類bwrapの診断を出して125で終わる
  And 説明文に "https://github.com/ba0918/kakoi/blob/main/docs/pasta.md" が含まれる

@id=EX-825 @about=REQ-429 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A6
Scenario: PATHにpastaが無いのに導入手順を示さないことは契約違反である
  Given ホストのPATHにpastaが無い
  When filteredで起動した診断を読む
  Then 説明文に導入手順の文書のURLが無いことは契約違反である

@id=EX-839 @about=REQ-429 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A22,docs/decision/brainstorm/2026-09-25-pasta-install.md#A6
Scenario: pastaとnftがともに無いときはpastaを報告する
  Given ホストのPATHにpastaもnftも無い
  When filteredで起動する
  Then 診断の説明文はpastaについて述べ、導入手順の文書のURLを含む

@id=EX-826 @about=REQ-430 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A1,docs/decision/brainstorm/2026-09-25-pasta-install.md#A4,docs/decision/brainstorm/2026-09-25-pasta-install.md#A18,docs/decision/brainstorm/2026-09-25-pasta-install.md#A5,docs/decision/brainstorm/2026-09-25-pasta-install.md#A20
Scenario: 古いpastaの足りないオプションを名指しする
  Given kakoiはpastaに長い名前で "--host-lo-to-ns-lo" と "--map-host-loopback" を渡し、PATHのpastaはこの2つを知らず、知らないオプションを受けると使い方の文を出して終了し、"--help" では使い方を標準出力に出して0で終わる
  When filteredで起動する
  Then 種類bwrapの診断を出して125で終わる
  And 説明文に "--host-lo-to-ns-lo" と "--map-host-loopback" と導入手順の文書のURLが含まれる

@id=EX-827 @about=REQ-430 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A1,docs/decision/brainstorm/2026-09-25-pasta-install.md#A22
Scenario: 古いpastaの出力を診断に含めない
  Given PATHのpastaは "--host-lo-to-ns-lo" を知らず、起動では "unrecognized option by test" と使い方の文を出して終了する
  When filteredで起動する
  Then 説明文に "unrecognized option by test" も使い方の文のどの行も含まれない

@id=EX-828 @about=REQ-430 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A10,docs/decision/brainstorm/2026-09-25-pasta-install.md#A20,docs/decision/brainstorm/2026-09-25-pasta-install.md#A1,docs/decision/brainstorm/2026-09-25-pasta-install.md#A5
Scenario: 足りると分かっていた長い名前でも確かめる
  Given PATHのpastaは "--no-map-gw" だけを知らず、起動では終了する
  When filteredで起動する
  Then 説明文に "--no-map-gw" が含まれる

@id=EX-840 @about=REQ-430 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A20,docs/decision/brainstorm/2026-09-25-pasta-install.md#A1,docs/decision/brainstorm/2026-09-25-pasta-install.md#A5
Scenario: 長い名前の一部だけの一致を載っているとみなさない
  Given PATHのpastaの "--help" は "--host-lo-to-ns-lo-extra" を載せるが "--host-lo-to-ns-lo" を載せず、起動では終了する
  When filteredで起動する
  Then 説明文に "--host-lo-to-ns-lo" が含まれる

@id=EX-841 @about=REQ-430 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A19,docs/decision/brainstorm/2026-09-25-pasta-install.md#A1,docs/decision/brainstorm/2026-09-25-pasta-install.md#A5,docs/decision/brainstorm/2026-09-25-pasta-install.md#A7,docs/decision/brainstorm/2026-09-25-pasta-install.md#A20
Scenario: パイプが先に閉じてから終了した古いpastaも見分ける
  Given kakoiはpastaに長い名前で "--map-host-loopback" を渡し、PATHのpastaはそれを知らず、起動用のパイプを閉じてから終了する
  When filteredで起動する
  Then 説明文に "--map-host-loopback" と導入手順の文書のURLが含まれる

@id=EX-829 @about=REQ-431 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A4
Scenario: 起動に成功したpastaにはhelpを求めない
  Given PATHのpastaはkakoiが渡すオプションをすべて受け付けて準備完了の合図を返し、呼ばれた引数を記録する
  When filteredで起動し、pastaの起動が成功する（その後の成否は問わない）
  Then その実行の間にpastaが "--help" 付きで呼ばれた記録は無い

@id=EX-830 @about=REQ-431 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A4
Scenario: 起動に成功したのにhelpを求めることは契約違反である
  Given PATHのpastaはkakoiが渡すオプションをすべて受け付けて準備完了の合図を返し、呼ばれた引数を記録する
  When filteredで起動し、pastaの起動が成功する（その後の成否は問わない）
  Then pastaが "--help" 付きで呼ばれていることは契約違反である

@id=EX-842 @about=REQ-431 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A19
Scenario: 起動のタイムアウトではhelpを求めない
  Given PATHのpastaは準備完了の合図を返さずに動き続け、呼ばれた引数を記録する
  When filteredで起動し、pastaの起動がタイムアウトする
  Then pastaが "--help" 付きで呼ばれた記録は無い

@id=EX-843 @about=REQ-431 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A22
Scenario: 計画表示ではpastaを確かめない
  Given filteredのポリシーで、PATHのpastaは呼ばれた引数を記録する
  When "--print-plan" で起動する
  Then pastaが呼ばれた記録は無い

@id=EX-831 @about=REQ-432 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A11,docs/decision/brainstorm/2026-09-25-pasta-install.md#A18,docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: オプションが揃ったpastaの起動の失敗は今の診断のままにする
  Given PATHのpastaの "--help" は標準出力にkakoiが長い名前で渡すオプションをすべて載せ、起動では "permission denied by test" を出して終了する
  When filteredで起動する
  Then 種類bwrapの診断を出して125で終わる
  And 説明文に "permission denied by test" が含まれ、導入手順の文書のURLは含まれない

@id=EX-832 @about=REQ-432 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A11,docs/decision/brainstorm/2026-09-25-pasta-install.md#A18,docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: helpの出力が空なら古いと決めつけない
  Given PATHのpastaは起動で "startup failed by test" を出して終了し、"--help" では何も出さずに0で終わる
  When filteredで起動する
  Then 種類bwrapの診断を出して125で終わる
  And 説明文に "startup failed by test" が含まれ、導入手順の文書のURLは含まれない

@id=EX-844 @about=REQ-432 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A18,docs/decision/brainstorm/2026-09-25-pasta-install.md#A11
Scenario: helpの終了コードでは判定しない
  Given PATHのpastaは起動で "startup failed by test" を出して終了し、"--help" では標準出力にkakoiが長い名前で渡すオプションをすべて載せて1で終わる
  When filteredで起動する
  Then 説明文に "startup failed by test" が含まれ、導入手順の文書のURLは含まれない

@id=EX-833 @about=REQ-433 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A8
Scenario: PATHの前に置いた新しいpastaを使う
  Given "~/.local/bin" に新しいpastaがあり、PATHでは "/usr/bin" の古いpastaより前に来る
  When filteredで起動する
  Then "~/.local/bin" のpastaを起動する

@id=EX-834 @about=REQ-433 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A8
Scenario: pastaの場所を選べる手段があることは契約違反である
  Given ポリシーと環境変数を読む処理を調べる
  When pastaの場所を指定する項目を探す
  Then その項目があることは契約違反である

@id=EX-835 @about=REQ-434 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A17,docs/decision/brainstorm/2026-09-25-pasta-install.md#A12,docs/decision/brainstorm/2026-09-25-pasta-install.md#A21,docs/decision/brainstorm/2026-09-25-pasta-install.md#A20
Scenario: 古いpastaしか無い利用者が文書の手順で入れ直す
  Given Ubuntu 24.04の標準のpastaしか無い利用者が "docs/pasta.md" を読む
  When 確かめ方に従い、足りなければソースビルドの手順を実行する
  Then "~/.local/bin" のpastaの "--help" に文書が載せた名前がすべて載る

@id=EX-836 @about=REQ-434 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A17,docs/decision/brainstorm/2026-09-25-pasta-install.md#A21,docs/decision/brainstorm/2026-09-25-pasta-install.md#A20
Scenario: 文書の名前の一覧がコードとずれることは契約違反である
  Given "docs/pasta.md" を読む
  When 契約への適合を確認する
  Then 確かめ方の名前の一覧がkakoiが長い名前で渡すオプションと一致しないこと、ディストリごとの版の一覧やpodman-staticから取り出す手順があることは契約違反である

@id=EX-837 @about=REQ-435 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A3,docs/decision/brainstorm/2026-09-25-pasta-install.md#A9,docs/decision/brainstorm/2026-09-25-pasta-install.md#A21
Scenario: filteredを使いたい利用者に古いpastaを知らせる
  Given 利用者がfilteredを使いたいと答え、導入済みのpastaの "--help" に "docs/pasta.md" の名前のうち "--host-lo-to-ns-lo" が無い
  When セットアップスキルがpastaを調べる
  Then "docs/pasta.md" の手順を示し、導入は利用者に任せる

@id=EX-838 @about=REQ-435 @source=docs/decision/brainstorm/2026-09-25-pasta-install.md#A3
Scenario: セットアップスキルがpastaを導入することは契約違反である
  Given 導入済みのpastaが古い
  When セットアップスキルが手順を示す
  Then スキルがパッケージの導入やビルドを実行することは契約違反である
```
