# シムの動作と検証

既存仕様から継承した本体の検査用表現。同じ責務を一文書にまとめ、見出しと要求IDで参照する。この IR が正本である。

## Requirements

### REQ-366: 汎用の本体と対象ごとの設定
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: examples/shim/codexがあり、READMEから指されていることを確かめる。skills/kakoi-setup/assets/shim/codexが同一内容の写しであり、その一致を検証する手段があることを確かめる。雛形のツール節が実体名、差し込むフラグ、workspace/rwへの引数の対応、素通し許可リスト、フラグなし一覧の5項目で構成され、本体部分がcodexに限定した処理を持たず、フラグを空にしても本体が動くことを確かめる。

examples/shim/codexに雛形を置きREADMEから指す。セットアップスキルにも同一内容の写しを含め一致を検証する。codexは例で、規則は対象を限定しない。ツール節は実体名、差込フラグ、workspace/rwへの引数対応、素通し許可リスト、フラグなし一覧の5項目で構成し、フラグが空でも本体が動く。

### REQ-367: 既定の隔離と承認済み例外
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: examples/shim/codexを読み、help/versionを含むすべての起動が既定でkakoiを通り、KAKOI_SHIM_OFF=1と素通し許可リストへの一致のときだけkakoiも追加フラグも付けずに実体をexecすること、モデルの起動の有無で本体が自動で分類する処理が無いことを確かめる。

シムはhelp/versionを含むすべての起動を既定でkakoiへ通す。例外はKAKOI_SHIM_OFF=1と利用者が承認した素通し許可リストで、どちらもkakoiも追加フラグも付けず実体をexecする。許可前に利用者は文書と実機観測でモデルが動かないことを確かめ、素通しの結果に責任を持つ。モデル起動の有無から本体が自動分類しない。

### REQ-368: 一覧の照合範囲と出荷値
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: examples/shim/codexを読み、素通し許可リストとフラグなし一覧を引数の先頭の語だけと照合し、先頭がオプションなら一致させず隔離すること、両一覧が出荷時に空であること、入れ子でもkakoiを呼び検出と警告をkakoiに任せることを確かめる。

素通し許可リストとフラグなし一覧は引数の先頭の語だけに照合し、先頭がオプションなら一致せず隔離する。両一覧は出荷時は空で、利用者の写しで壊れたときだけ増やす。入れ子でもkakoiを呼び、検出・警告をkakoiに任せる。

### REQ-369: 壊れ方に応じた案内
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: examples/shim/codexのヘッダーを読み、フラグの拒否・無効ならフラグなし一覧、ホスト到達が必要でモデルが動かないなら承認後の許可リスト、モデルが動くなら一覧へ足さずプロファイルを直す、という表があること、認証サブコマンドが未実測の読み方の例として示されていることを確かめる。本体に実行時にヒントを出力する処理が無いことを確かめる。

雛形のヘッダーはフラグ拒否・無効ならフラグなし一覧、ホスト到達が必要かつモデルが動かないなら承認後の許可リスト、モデルが動くなら一覧へ足さずプロファイル修正とする表を持つ。認証サブコマンドを未実測の読み方の例として示す。実行時にヒントを出さない。

### REQ-370: 実体の探索
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: examples/shim/codexを読み、PATHから自分を除いて実体を探し、見つけた候補パスをそのままCOMMANDにすること、候補のリンクを最後まで解決せず自分との同一性の比較のときだけ実体化することを確かめる。REQ-373の代役の手順で、版管理ツールのshimsのリンクを候補にしたとき、kakoiの代役が受け取るCOMMANDがその候補パスそのものであることを観測する。

シムはPATHから自分を除いて実体を探し、見つけた候補パスをそのままCOMMANDにする。候補のリンクは最後まで解決せず、自分との同一性比較だけ実体化する。名前だけ渡してシムへ戻ることや版管理ツールの名前を失うことを防ぐ。

### REQ-371: 共有ディレクトリの作成
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: examples/shim/codexを読み、kakoi経由の経路でだけ/tmp/kakoiが無ければ作ること、その名前がリンクなら作らず停止すること、素通しの経路では作らないことを確かめる。

kakoi経由の経路だけ/tmp/kakoiがなければ作る。その名前がリンクなら作らず停止する。素通し経路では作らない。

### REQ-372: フラグと引数の写し
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A2
- verification: review
- how_to_verify: examples/shim/codexの引数を走査する部分とヘッダーを読み、下の規則と一致することを確かめる。REQ-373の代役の手順で "codex -m --cd /etc exec" を起動し、kakoiの代役が "--workspace /etc" を受け取ることを観測する。

追加フラグはグローバルオプション位置である引数列の先頭へ置く。フラグなし一覧に一致してもkakoiは経由する。引数はツール節の対応に従い次の語・イコール結合・短縮結合の文法でworkspace/rwへ写し、--以後は走査しない。不在の写し先を雛形で検査しない。位置引数と空白区切り複数値の先頭以外は写さない。

雛形は包む対象の他のオプションが値を取るかを知らないので、写す対象のオプションと同じ名前の語を、他のオプションの値や位置引数の位置を含めてどこにあってもそのオプションとして読み、その値を写す。このため "--workspace" や "--rw" が利用者の意図より広がりうる（"codex -m --cd /etc exec" ではkakoiに "--workspace /etc" が渡る）。これは受け入れた性質である。

### REQ-373: シムの人による検証
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1, docs/decision/brainstorm/2026-09-25-u5-and-review-checks.md#A4
- verification: review
- how_to_verify: "docs/shim.md" を読み、PATH上の2つのディレクトリと引数を表示する代役を使う確認の手順と確認者が載っていることを確かめる。その手順に先頭がサブコマンド・オプション・helpの起動、引数の写し、一覧の先頭一致、空のフラグ、OFFの素通しの観測があり、同梱フラグの実対象への効果は保守側が、各許可項目でモデルが動かないことは利用者が確かめると書かれていることを確かめる。雛形が自動テストの対象に含まれていないことをテストの構成で確かめる。

雛形は第15節の自動テスト対象にせず、人がPATH上の2ディレクトリと引数表示の代役で確認する。先頭がサブコマンド・オプション・help、引数写し、一覧の先頭一致、空フラグ、OFFの素通しを観測する。同梱フラグの実対象への効果を保守側が確認し、各許可項目でモデルが動かないことは利用者が確認する。手順と確認者を"docs/shim.md"へ載せる。

### REQ-407: シムの詳細文書の掲載項目
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A17
- verification: review
- how_to_verify: "docs/shim.md" を読み、本文に挙げた5つの項目がそれぞれ載っていることを確かめる。

"docs/shim.md" には、雛形を写してツール節を埋める手順、同梱のツール節の値がcodexの例であること、REQ-369の壊れ方に応じた表、素通し許可リストに最初に入りそうな例、保守側と利用者のそれぞれが確かめる条件を載せる。

### REQ-408: READMEのシムの節
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A17
- verification: review
- how_to_verify: READMEのシムの節を読み、雛形を写す最短の手順、KAKOI_SHIM_OFF=1の存在、"docs/shim.md" へのリンクがあることを確かめる。

READMEのシムの節には、雛形を写す最短の手順、KAKOI_SHIM_OFF=1の存在、"docs/shim.md" への導線を載せる。

## Examples

```gherkin
@id=EX-672 @about=REQ-366 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 汎用の本体と対象ごとの設定・成功
  Given ツール節のフラグを空にする
  When 契約への適合を確認する
  Then 同じ本体で何も差し込まず起動する

@id=EX-673 @about=REQ-366 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 汎用の本体と対象ごとの設定・反例
  Given ツール節のフラグを空にする
  When 契約への適合を確認する
  Then codex以外では規則を適用しないことは契約違反である

@id=EX-674 @about=REQ-367 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 既定の隔離と承認済み例外・成功
  Given 未知のサブコマンドまたは--helpで起動する
  When 契約への適合を確認する
  Then kakoiを経由する

@id=EX-675 @about=REQ-367 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 既定の隔離と承認済み例外・反例
  Given 未知のサブコマンドまたは--helpで起動する
  When 契約への適合を確認する
  Then 知らないサブコマンドを素通しすることは契約違反である

@id=EX-676 @about=REQ-368 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 一覧の照合範囲と出荷値・成功
  Given 一覧の語の前にオプションを置く
  When 契約への適合を確認する
  Then 既定の隔離経路になる

@id=EX-677 @about=REQ-368 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 一覧の照合範囲と出荷値・反例
  Given 一覧の語の前にオプションを置く
  When 契約への適合を確認する
  Then 引数全体から語を探して素通しすることは契約違反である

@id=EX-678 @about=REQ-369 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 壊れ方に応じた案内・成功
  Given モデルが動くコマンドが隔離内で壊れる
  When 契約への適合を確認する
  Then プロファイルで直すよう案内する

@id=EX-679 @about=REQ-369 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 壊れ方に応じた案内・反例
  Given モデルが動くコマンドが隔離内で壊れる
  When 契約への適合を確認する
  Then 壊れたことだけを根拠に素通しへ加えることは契約違反である

```

```gherkin
@id=EX-680 @about=REQ-370 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 実体の探索・成功
  Given 候補が版管理ツールのシムである
  When 契約への適合を確認する
  Then 候補パスを保持してkakoiに渡す

@id=EX-681 @about=REQ-370 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 実体の探索・反例
  Given 候補が版管理ツールのシムである
  When 契約への適合を確認する
  Then 実体を最後まで解決して別名のツールを起動することは契約違反である

@id=EX-682 @about=REQ-371 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 共有ディレクトリの作成・成功
  Given /tmp/kakoiにリンクが置かれている
  When 契約への適合を確認する
  Then 隔離経路は停止してリンク先に作成しない

@id=EX-683 @about=REQ-371 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 共有ディレクトリの作成・反例
  Given /tmp/kakoiにリンクが置かれている
  When 契約への適合を確認する
  Then 他ユーザーが置いたリンクを辿って作ることは契約違反である

@id=EX-684 @about=REQ-372 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: フラグと引数の写し・成功
  Given 対応オプションを--以後に置く
  When 契約への適合を確認する
  Then その語をkakoi側の許可追加に使わない

@id=EX-685 @about=REQ-372 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: フラグと引数の写し・反例
  Given 対応オプションを--以後に置く
  When 契約への適合を確認する
  Then 位置引数を勝手にworkspaceへ写して許可を広げることは契約違反である

@id=EX-686 @about=REQ-373 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: シムの人による検証・成功
  Given 雛形の一般経路を代役で確認する
  When 契約への適合を確認する
  Then 本物の対象を起動せず引数と分岐を観測できる

@id=EX-687 @about=REQ-373 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: シムの人による検証・反例
  Given 雛形の一般経路を代役で確認する
  When 契約への適合を確認する
  Then 代役の成功だけで実対象へのフラグ効果も確認済みにすることは契約違反である

@id=EX-767 @about=REQ-372 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A2
Scenario: 同じ名前の語の読み取り・成功
  Given 写す対象に--cdがあり、利用者が-mの値の位置に--cdを置いて-m --cd /etc execと起動する
  When 契約への適合を確認する
  Then --cdをオプションとして読みkakoiに--workspace /etcを渡す

@id=EX-768 @about=REQ-372 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A2
Scenario: 同じ名前の語の読み取り・反例
  Given 写す対象に--cdがあり、利用者が-mの値の位置に--cdを置いて-m --cd /etc execと起動する
  When 契約への適合を確認する
  Then 写しの取りこぼしも誤読も隔離が緩む向きに働かないと説明することは契約違反である

@id=EX-769 @about=REQ-407 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A17
Scenario: シムの詳細文書の掲載項目・成功
  Given codex以外のコマンドを包みたい利用者が詳細文書を読む
  When 契約への適合を確認する
  Then ツール節を埋める手順と同梱の値がcodexの例であることが分かる

@id=EX-770 @about=REQ-407 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A17
Scenario: シムの詳細文書の掲載項目・反例
  Given codex以外のコマンドを包みたい利用者が詳細文書を読む
  When 契約への適合を確認する
  Then 壊れ方に応じた表を雛形のヘッダーにだけ置き詳細文書に載せないことは契約違反である

@id=EX-771 @about=REQ-408 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A17
Scenario: READMEのシムの節・成功
  Given ビルド環境を持つ利用者がREADMEのシムの節を読む
  When 契約への適合を確認する
  Then 雛形を写す最短の手順とKAKOI_SHIM_OFF=1を知り、"docs/shim.md" へ移れる

@id=EX-772 @about=REQ-408 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A17
Scenario: READMEのシムの節・反例
  Given READMEのシムの節から詳細文書へリンクしている
  When 契約への適合を確認する
  Then READMEのシムの節にKAKOI_SHIM_OFF=1を載せないことは契約違反である

```
