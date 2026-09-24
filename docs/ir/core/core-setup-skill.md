# セットアップスキルの契約

既存仕様から継承した本体の検査用表現。同じ責務を一文書にまとめ、見出しと要求IDで参照する。この IR が正本である。

## Requirements

### REQ-374: スキルの配置と配布
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "skills/kakoi-setup/SKILL.md" のfrontmatterを読み、nameがディレクトリ名と同じkakoi-setupでdescriptionを持つことを確かめ、agentskills validateでこのスキルが通ることを観測する。同梱物に独自のインストール手段が無く、READMEにgh skill installによる導入が書かれていることを確かめる。

skills/kakoi-setup/SKILL.mdにAgent Skills共通仕様に従うスキルを置く。nameはディレクトリと同じkakoi-setup、descriptionを持つ。agentskills validateで形式を確認し、独自のインストール手段を作らずREADMEにgh skill installを書く。

### REQ-375: 正本と未作成プロファイルの確認
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "skills/kakoi-setup/SKILL.md" を読み、プロファイルは開始時に、シムは置き場の承認時に、最初の提案の前に生成・同期元やリンク先という別の正本があるかを利用者に聞き、ファイルの印や内容から推測しないと書かれていることを確かめる。プロファイルが未作成ならinitの実行を尋ねて承認後にだけ実行し、実行しない場合はプロファイルを提案せず理由を伝えると書かれていることを確かめる。

プロファイルとシムそれぞれへの最初の提案前に、生成・同期元やリンク先という別の正本があるか利用者に聞く。プロファイルは開始時、シムは置き場承認時に聞き、ファイルの印や内容から推測しない。プロファイル未作成ならinit実行を尋ね、承認後だけ実行する。実行しない場合はプロファイルの提案をせず理由を伝える。

### REQ-376: 対象に合わせた提案
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "skills/kakoi-setup/SKILL.md" を読み、導入済みの対象を調べて状態の置き場のrwと設定・指示のroを提案する手順があることを確かめる。シムは同梱の写しからツール節を埋め、値は対象のhelp出力だけを根拠にして未実測と明記し実測は利用者が行うこと、記憶から雛形を書き起こさないこと、本体部分を同梱の写しでも利用者の写しでも編集しないことが書かれていることを確かめる。

導入済みの対象を調べ、状態置き場のrwと設定・指示のroを提案する。シムは同梱の写しからツール節を埋め、値は対象のhelp出力だけを根拠にして未実測と明記し、実測は利用者が行う。記憶から雛形を書き起こさず、本体部分は同梱・利用者の写しとも編集しない。

### REQ-377: 引数写しの制限の案内
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "skills/kakoi-setup/SKILL.md" を読み、位置引数の作業場所は写らずcwdのままなのでその場所での起動を案内すること、空白区切りの複数値は先頭だけ写り、rwなら対象が許す場合のオプションの反復、workspaceなら最後の1つだけなので現地での起動を案内することが書かれていることを確かめる。区切り文字で1語にした値は全体が不在のパスになりrwは飛ばされworkspaceはpathで止まること、該当するオプションと理由を示して黙って落とさないことが書かれていることを確かめる。

位置引数の作業場所は写らずcwdのままになるので、その場所での起動を案内する。空白区切りの複数値は先頭だけ写り、rwなら対象が許す場合のオプション反復、workspaceなら最後の1つのみのため現地起動を案内する。区切り文字で1語にした値は全体が不在パスとなり、rwは飛ばされworkspaceはpathで止まる。該当オプションと理由を示し黙って落とさない。

### REQ-378: 差分を示してから書く
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "skills/kakoi-setup/SKILL.md" を読み、プロファイルとシムは書き先と位置を差分で示して利用者の承認後に書くこと、利用者が内容を先に指定した場合もCLI側に書き込みの承認がある場合もこの提示を省かないこと、スキルが隔離の外で動くことが書かれていることを確かめる。

プロファイルとシムは書き先・位置を差分で示し利用者が承認した後に書く。利用者が内容を先に指定した場合もCLI側の書込承認がある場合もこの提示を省かない。スキルは隔離外で動く。

### REQ-379: 書き先と別の正本の境界
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "skills/kakoi-setup/SKILL.md" を読み、書き先が設定ディレクトリのsecrets以外と承認したシムの置き場に限られること、別の正本があると利用者が答えた場所には書かずプロファイルなら行、シムなら内容と実行可能ビットを利用者へ提案すること、仕様と公開文書を編集しないことが書かれていることを確かめる。

スキルの書き先は設定ディレクトリのsecrets以外と承認したシム置き場に限る。別の正本があると利用者が答えた場所には書かず、プロファイルなら行、シムなら内容と実行可能ビットを利用者へ提案する。仕様と公開文書は編集しない。

### REQ-380: 読めない設定の扱い
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "skills/kakoi-setup/SKILL.md" を読み、設定ディレクトリを読めない場合はfull計画の合成後ポリシーと出所を読んでコメントは見えていないと伝えること、組み込みの既定や記憶で補わないこと、書ける場合でもプロファイルには書かず行を提案することが書かれていることを確かめる。

設定ディレクトリを読めない場合はfull計画の合成後ポリシーと出所を読み、コメントは見えていないと伝える。組み込み既定や記憶で補わない。差分を作れないため書ける場合でもプロファイルには書かず行を提案する。

### REQ-381: 秘密への非アクセス
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "skills/kakoi-setup/SKILL.md" を読み、secrets配下を一覧・読み取り・書き込みせず置き場とモードだけを案内すること、保護対象が値とファイル名の一覧でありプロファイルや計画に既に現れる環境変数名とパスを含まないことが書かれていることを確かめる。secrets項目があるとき、秘密の不在を知らせる警告なら不在、secret診断なら存在するが値は使えない、どちらも無ければ存在と判断し、項目が無ければ利用者に尋ねると書かれていることを確かめる。

スキルはsecrets配下を一覧・読取・書込せず置き場とモードだけ案内する。保護対象は値とファイル名一覧であり、既にプロファイルや計画に現れる環境変数名・パスは含まない。secrets項目があれば秘密の不在を知らせる警告は不在、secret診断は存在するが値不可、警告も診断もなければ存在と判断する。項目がなければ利用者に尋ねる。

### REQ-382: 実行可能ビットと経路の確認
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "skills/kakoi-setup/SKILL.md" を読み、シムの差分と同時に実行可能にすると告知して設置後にモードを確かめること、利用者が挙げた起動経路のうち最低限対話シェルとエージェントCLIの両方でcommand -v NAMEを実体化し承認した写しと一致するかを確かめることが書かれていることを確かめる。CLI側はスキルが実行し到達できない経路は利用者が確認すること、対象コマンドそのものを起動しないこと、対話シェルの関数とaliasによる横取りも確認することが書かれていることを確かめる。

シムの差分と同時に実行可能にすると告知し、設置後にモードを確かめる。利用者が起動経路を挙げ、最低限対話シェルとエージェントCLIの両方でcommand -v NAMEを実体化し承認した写しに一致することを確認する。CLI側はスキルが実行し、到達できない経路は利用者が確認する。対象コマンドそのものは起動せず、対話シェルの関数・aliasの横取りも確認する。

### REQ-383: PATHの不一致と隔離内PATHの区別
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "skills/kakoi-setup/SKILL.md" を読み、CLI側で写しが見つからないときにPATH変更の未反映の可能性を示してCLIの再起動後に再確認すること、版管理ツールのshimsが前にあると素通りし対話・非対話で順が違う場合があることを案内すること、環境固有の代替コマンドのpath-prependは隔離内のPATHへの提案でありホストの探索順と分けることが書かれていることを確かめる。

CLI側で写しを見つけない場合はPATH変更が未反映の可能性を示し、CLI再起動後に再確認する。版管理ツールのshimsが前にあると素通りし、対話・非対話で順が違う場合があることを案内する。環境固有の代替コマンドのpath-prependは隔離内PATHへの提案でありホストの探索順と分ける。

### REQ-384: 計画で反映を確認
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "skills/kakoi-setup/SKILL.md" を読み、変更前後のprint-planを比較すること、別の正本の場合は利用者が反映したと伝えてから変更後を取得すること、前後ともfull計画で提案項目が合成後に現れるかを確認し不在なら報告することが書かれていることを確かめる。隔離内で壊れたコマンドにはシムの分類表に沿って一覧かプロファイルの差分を提案し判断を利用者へ渡すと書かれていることを確かめる。

変更前後のprint-planを比較する。別の正本の場合は利用者が反映したと伝えてから変更後を取得し、前後ともfull計画で提案項目が合成後に現れることを確認し、不在なら報告する。隔離内で壊れたコマンドはシムの分類表に沿って一覧かプロファイルの差分を提案し、判断は利用者へ渡す。

### REQ-385: スキルの人による検証
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "skills/kakoi-setup/SKILL.md" が自動テストの対象に含まれていないことをテストの構成で確かめる。通常と内容先指定での差分承認、最初の提案前の正本の質問、未作成時のinitの確認、別の正本への非書き込み、写せない引数の説明、設置後の経路の確認、設定が読めないときのコメント不明の告知について、スキルに指示があることを読み、人がスキルを実際に試行してそれぞれを観測する。

スキルは第15節のテスト対象にせず、人が指示の存在と実際の試行を確認する。通常・内容先指定での差分承認、最初の提案前の正本質問、未作成時のinit確認、別正本への非書込、写せない引数の説明、設置後の経路確認、設定不可読時のコメント不明の告知を観測する。

### REQ-405: initが書き出す内容の案内
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A16
- verification: review
- how_to_verify: SKILL.mdのinitの実行を尋ねる手順が、書き出す内容を組み込みの既定そのものでありfull計画の合成後ポリシーと同じだと示すことを読む。設定ディレクトリが無い状態の kakoi --print-plan=full -- true の合成後ポリシーと、その後にinitを実行して同じ計画を出したときの合成後ポリシーが一致することを確かめる。

initが書き出すのは組み込みの既定そのもので、他の段が無いときのfull計画の合成後ポリシーと同じ内容である。スキルはinitの実行を尋ねるとき、書き出す内容をこの形で利用者に示す。

### REQ-406: エージェントCLI以外の対象
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A16
- verification: review
- how_to_verify: SKILL.mdの導入済みのコマンドを調べる手順が、対象をエージェントCLIに限らず利用者が隔離の中で動かしたいコマンドとしていることを読む。

スキルは導入済みの対象コマンドを調べてプロファイルの項目を提案する。対象は利用者が隔離の中で動かしたいコマンドで、エージェントCLI以外も含む。

## Examples

```gherkin
@id=EX-688 @about=REQ-374 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: スキルの配置と配布・成功
  Given スキルの形式を検証する
  When 契約への適合を確認する
  Then nameとディレクトリ名が一致しdescriptionがある

@id=EX-689 @about=REQ-374 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: スキルの配置と配布・反例
  Given スキルの形式を検証する
  When 契約への適合を確認する
  Then 専用インストーラを追加することは契約違反である

@id=EX-690 @about=REQ-375 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 正本と未作成プロファイルの確認・成功
  Given プロファイルが存在しない
  When 契約への適合を確認する
  Then initの実行を確認してから進める

@id=EX-691 @about=REQ-375 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 正本と未作成プロファイルの確認・反例
  Given プロファイルが存在しない
  When 契約への適合を確認する
  Then 既定内容を想定してファイルへ書く提案を先に出すことは契約違反である

@id=EX-692 @about=REQ-376 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 対象に合わせた提案・成功
  Given 対象のhelpから対応表を提案する
  When 契約への適合を確認する
  Then 根拠と未実測であることを示す

@id=EX-693 @about=REQ-376 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 対象に合わせた提案・反例
  Given 対象のhelpから対応表を提案する
  When 契約への適合を確認する
  Then 記憶したフラグを実測済みとして加えることは契約違反である

@id=EX-694 @about=REQ-377 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 引数写しの制限の案内・成功
  Given 対象のhelpが位置引数で作業場所を受ける
  When 契約への適合を確認する
  Then 写せない理由とその場所で起動する回避策を示す

@id=EX-695 @about=REQ-377 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 引数写しの制限の案内・反例
  Given 対象のhelpが位置引数で作業場所を受ける
  When 契約への適合を確認する
  Then 対応したつもりでcwdに許可を置くことは契約違反である

```

```gherkin
@id=EX-696 @about=REQ-378 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 差分を示してから書く・成功
  Given 利用者が変更行を先に指定する
  When 契約への適合を確認する
  Then 書く前に差分提示と承認要求を行う

@id=EX-697 @about=REQ-378 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 差分を示してから書く・反例
  Given 利用者が変更行を先に指定する
  When 契約への適合を確認する
  Then 先に書いてから差分を見せることは契約違反である

@id=EX-698 @about=REQ-379 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 書き先と別の正本の境界・成功
  Given プロファイルが他所の生成物と分かる
  When 契約への適合を確認する
  Then 利用者が正本へ反映する行だけ提案する

@id=EX-699 @about=REQ-379 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 書き先と別の正本の境界・反例
  Given プロファイルが他所の生成物と分かる
  When 契約への適合を確認する
  Then 承認を取って生成物や外部の正本へ書くことは契約違反である

@id=EX-700 @about=REQ-380 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 読めない設定の扱い・成功
  Given 設定を直接読めず計画だけ読める
  When 契約への適合を確認する
  Then コメント不明を伝え行の提案にとどめる

@id=EX-701 @about=REQ-380 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 読めない設定の扱い・反例
  Given 設定を直接読めず計画だけ読める
  When 契約への適合を確認する
  Then 既定ファイルの内容を現在の設定だと提示することは契約違反である

@id=EX-702 @about=REQ-381 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 秘密への非アクセス・成功
  Given 既定のままでsecrets項目がない
  When 契約への適合を確認する
  Then 配置状況を利用者に聞く

@id=EX-703 @about=REQ-381 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 秘密への非アクセス・反例
  Given 既定のままでsecrets項目がない
  When 契約への適合を確認する
  Then 確認のためsecretsディレクトリを一覧することは契約違反である

```

```gherkin
@id=EX-704 @about=REQ-382 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 実行可能ビットと経路の確認・成功
  Given シムの写しをPATHに置く
  When 契約への適合を確認する
  Then 実行可能であり各経路の探索結果が承認した写しを指す

@id=EX-705 @about=REQ-382 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 実行可能ビットと経路の確認・反例
  Given シムの写しをPATHに置く
  When 契約への適合を確認する
  Then まだ設置していない写しを有効と報告することは契約違反である

@id=EX-706 @about=REQ-383 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: PATHの不一致と隔離内PATHの区別・成功
  Given 対話シェルだけ写しを見つける
  When 契約への適合を確認する
  Then CLI側も再起動して確認する

@id=EX-707 @about=REQ-383 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: PATHの不一致と隔離内PATHの区別・反例
  Given 対話シェルだけ写しを見つける
  When 契約への適合を確認する
  Then 隔離内path-prependだけでホストのシム探索を直したとすることは契約違反である

@id=EX-708 @about=REQ-384 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 計画で反映を確認・成功
  Given 別の正本への変更を提案する
  When 契約への適合を確認する
  Then 利用者の反映報告後にfull計画で確認する

@id=EX-709 @about=REQ-384 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 計画で反映を確認・反例
  Given 別の正本への変更を提案する
  When 契約への適合を確認する
  Then 提案を出しただけで反映済みとすることは契約違反である

@id=EX-710 @about=REQ-385 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: スキルの人による検証・成功
  Given 内容を先に指定した試行を確認する
  When 契約への適合を確認する
  Then 指示文だけでなく差分承認を実際に観測する

@id=EX-711 @about=REQ-385 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: スキルの人による検証・反例
  Given 内容を先に指定した試行を確認する
  When 契約への適合を確認する
  Then SKILL.mdに指示があるだけで遵守したとすることは契約違反である

@id=EX-763 @about=REQ-405 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A16
Scenario: initが書き出す内容の案内・成功
  Given プロファイルが存在せずinitの実行を尋ねる
  When 契約への適合を確認する
  Then 組み込みの既定そのものでfull計画の合成後ポリシーと同じ内容を書くと伝える

@id=EX-764 @about=REQ-405 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A16
Scenario: initが書き出す内容の案内・反例
  Given プロファイルが存在せずinitの実行を尋ねる
  When 契約への適合を確認する
  Then 組み込みの既定と異なる内容を書くと案内することは契約違反である

@id=EX-765 @about=REQ-406 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A16
Scenario: エージェントCLI以外の対象・成功
  Given 利用者がエージェントCLIでないコマンドも隔離の中で動かしたい
  When 契約への適合を確認する
  Then そのコマンドの状態置き場と設定も提案の対象にする

@id=EX-766 @about=REQ-406 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A16
Scenario: エージェントCLI以外の対象・反例
  Given 利用者がエージェントCLIでないコマンドも隔離の中で動かしたい
  When 契約への適合を確認する
  Then エージェントCLIでないことを理由に提案から外すことは契約違反である

```
