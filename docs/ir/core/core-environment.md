# 環境変数と秘密の構成

既存仕様から継承した本体の検査用表現。同じ責務を一文書にまとめ、見出しと要求IDで参照する。この IR が正本である。

## Requirements

### REQ-268: 環境の7段階
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

環境をinheritのホスト全体またはclearのpass対象だけから始め、unset、set、secrets、git書き換え、PATH先頭追加、KAKOI=1の順に組み立てる。

### REQ-269: 環境の受け渡し
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

組み立てた環境はbwrapに環境として引き継がせ、環境の値をbwrap引数に載せない。マウントと秘密ファイルのパスが引数に現れること、信頼するホストが/procから環境を読めることは許容する。

### REQ-270: PATHの先頭追加
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

path-prependを実体パスで先頭に加え、存在しない項目を理由付きで計画に載せて飛ばす。既存PATHがなければ追加分だけとし、追加分もなければPATH不在のまま警告しない。

### REQ-271: 秘密の置換と欠落
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

secretsを環境変数名のバイト順に処理し、各名前の既存値を由来にかかわらず消してから、存在するファイルの値を入れてファイルをhideする。不在なら変数不在のままwarningを標準エラーに1行出し続行する。

### REQ-272: 秘密の末尾改行
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

秘密ファイルから末尾のLF1バイトまたはCRLF2バイトを改行1つとして一度だけ除去する。

### REQ-273: 秘密の値の拒否
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

改行除去後が空、NULを含む、64KiB超、または読めない秘密をsecret診断にする。64KiBの値は許し、この値の上限は一般の読込上限より先に効く。

### REQ-274: 秘密を表示しない
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

計画、警告、診断に秘密の値を含めない。表示は変数名とファイルパスに限り、値を伏せる。秘密から得たGIT_CONFIG_COUNTが不正でも値を診断に出さない。

### REQ-275: 継承した認証情報
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

inheritではホストの認証情報も継承する。名前に対するenv.unsetのワイルドカードが一致しない認証情報は残る。この隙間をREADMEに記す。

### REQ-276: Git設定の追加
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

git.instead-ofは元URL先頭をキー、置換先を値とし、キーのバイト順でurl.<置換先>.insteadofを元先頭の値で環境へ追加する。secrets適用後のCOUNTの続きから番号を振り、不在なら0から始め、既存の組を保持する。

### REQ-277: Gitの番号検査
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

git.instead-ofがある場合だけCOUNTを検査する。空、符号付き、空白入り、非数値、桁超過、追加による番号上限超過はenv診断。項目がない場合はCOUNTを検査しない。

### REQ-278: 環境値を使う合成の限定
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

他の環境変数の値を決めるために既存値を読む合成は、GitのCOUNTとPATH先頭追加だけとする。

## Examples

```gherkin
@id=EX-505 @about=REQ-268 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 環境の7段階
  Given 本体の既存仕様を適用する
  When unsetとsetで同じ名前を指定する
  Then setの値が最終環境に入る

@id=EX-506 @about=REQ-269 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 環境の受け渡し
  Given 本体の既存仕様を適用する
  When 秘密を含む環境の起動引数を作る
  Then 環境設定の引数も秘密の値も含めない

@id=EX-507 @about=REQ-270 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: PATHの先頭追加
  Given 本体の既存仕様を適用する
  When clearでPATHを渡さず追加分もない
  Then PATHは不在で警告しない

@id=EX-508 @about=REQ-271 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 秘密の置換と欠落
  Given 本体の既存仕様を適用する
  When ホストTOKENがあり秘密ファイルがない
  Then TOKENを残さずwarningを出して続行する

@id=EX-509 @about=REQ-272 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 秘密の末尾改行
  Given 本体の既存仕様を適用する
  When 秘密ファイルがvと2個のLFを持つ
  Then 環境の値はvと1個のLFとなる

```

```gherkin
@id=EX-510 @about=REQ-273 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 秘密の値の拒否
  Given 本体の既存仕様を適用する
  When 改行除去後の秘密が65537バイトである
  Then secretで125となる

@id=EX-511 @about=REQ-274 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 秘密を表示しない
  Given 本体の既存仕様を適用する
  When 秘密のGIT_CONFIG_COUNTが非数値である
  Then env診断にその値を含めない

@id=EX-512 @about=REQ-275 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 継承した認証情報
  Given 本体の既存仕様を適用する
  When unsetを*_TOKENとしてTOKENを継承する
  Then TOKENは一致しないので残る

@id=EX-513 @about=REQ-276 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: Git設定の追加
  Given 本体の既存仕様を適用する
  When COUNTが1で既存KEY_0とVALUE_0がある
  Then 既存組を保ち追加分を1から振る

@id=EX-514 @about=REQ-277 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: Gitの番号検査
  Given 本体の既存仕様を適用する
  When COUNTが空でgit.instead-ofがない
  Then 空のCOUNTをそのまま保持する

```

```gherkin
@id=EX-515 @about=REQ-153 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 環境値を使う合成の限定
  Given 本体の既存仕様を適用する
  When env.setに${worktree}という文字列がある
  Then 環境の値には文字列をそのまま入れる

```
