# 起動計画の表示と検査順序

既存仕様から継承した本体の検査用表現。同じ責務を一文書にまとめ、見出しと要求IDで参照する。この IR が正本である。

## Requirements

### REQ-294: マウント段階内の順序
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

段階7は同一性、scan rootとhide-mounts underの事前検査、生成、種類検査、配置保護・根・露出組・固定部分・広さの検査、位置とcwd検査、秘密読込、Git COUNT、rw-copy読込の順に検査する。同一性はpolicyまたはCLIのusage、秘密はsecret、COUNTはenv、他はpath。

### REQ-295: 同段階の診断順序
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

同検査のマウントは適用順、環境と秘密は名前のバイト順で最初を診断する。保護対象はprofile、policy-file、設定ディレクトリ、秘密名順、合成後path-prepend順。scan rootがunderより先、生成前の検査が書かれた項目より先。根と露出組は項目順の後workspaceを見て、2検査の交互配置は委譲する。

### REQ-296: 要約の内容
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

要約はポリシーの実体パス、4変数、通信モード、マウント適否と理由、環境のホストとの差分、解決コマンドを表示する。組込み既定の欄にkakoi initを含め、マウントのHOMEを~に縮め、非グローバル由来だけ出所を添える。同値環境は件数、秘密は名前だけとする。

### REQ-297: 全量と共通表示
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

全量は合成後ポリシー、マウント実体パスと出所、秘密を伏せた全環境、解決コマンド、bwrap引数を示す。要約と全量は飛ばした項目と理由、複製できなかったエントリと理由、rw-copyがホスト内容で始まり書込みがホストに出ず終了時消える説明、入れ子の印を共通して持つ。

### REQ-298: JSONの外形と版
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

JSON計画は全量に環境差分を加えた1行1文書を末尾LF1個付きで出す。format_versionは整数1から始め、キー削除と意味変更で上げ、追加だけでは上げない。同じ版で既存キーと意味を保つ。

### REQ-299: JSONの主要キー
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

JSONはformat_version、nested、policy_sources、variables、home、policy、mounts、skipped_mounts、left_visible、skipped_paths、not_copied、environment、environment_changes、command、bwrap、bwrap_argumentsを持つ。variablesは4変数、不在値はnull。COMMAND省略時commandはnull。mountsは実体パス・種類・記述値・出所を持つ。

### REQ-300: JSONの秘密と記述子
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

JSONの秘密値はenvironmentでnullとし他のキーにも値を出さない。bwrap_argumentsはliteralとvalue、seccomp-filter、empty-file、copied-fileと長さbytesで表しコピー内容を出さない。not_copiedは項目実体パス・エントリパス・理由を持つ。

### REQ-301: JSONの出所と文字
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: unit

JSONの項目の出所はprofile、built-in-default、policy-file、command-line、scan、hide-mounts、secret、config-secretsのkindを持ち、ファイルがあるものはpath、secretはnameを持つ。不正UTF-8は置換文字、制御文字はJSONエスケープとし生の制御文字を出さない。

### REQ-403: 生成の段の検査
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A12
- verification: unit

REQ-294の3番目の生成の段では、生成に加えて次を検査し、どちらも種類pathの診断で終わる。走査で一致したシンボリックリンクの先が書かれたroの項目の中にあり、そのroの項目が隔離の中から差し替えられる（解決が書き込める項目を通る）とき。mounts.hide-mountsが書かれていて、マウント一覧を読めないとき。

### REQ-404: rw-copyの読み込みの検査
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A12
- verification: unit

REQ-294の9番目のrw-copyの読み込みの段では、適用するrw-copyの項目ごとに、対象の種類、読めない複製元、量の上限を検査し、どれも種類pathの診断で終わる。項目の実体がディレクトリでも通常ファイルでもないとき、一覧できないディレクトリか読めないファイルがあるとき、REQ-311の上限（項目あたり4096エントリ、通常ファイルの内容の合計64MiB）を超えるときである。

## Examples

```gherkin
@id=EX-530 @about=REQ-294 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: マウント段階内の順序
  Given 本体の既存仕様を適用する
  When 同一性衝突、配置違反、空の秘密がある
  Then 同一性衝突の診断で止まる

@id=EX-531 @about=REQ-295 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 同段階の診断順序
  Given 本体の既存仕様を適用する
  When 走査で一致した不正リンクがb、aの順で見つかる
  Then 名前のバイト順で先のaを診断する

@id=EX-532 @about=REQ-296 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 要約の内容
  Given 本体の既存仕様を適用する
  When ホストと同じKEPTと追加NEWを持つ計画を見る
  Then KEPTは件数に含めNEWの値を示す

@id=EX-533 @about=REQ-297 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 全量と共通表示
  Given 本体の既存仕様を適用する
  When rw-copyを含む全量計画を見る
  Then 複製の性質と複製できなかった項目を表示する

@id=EX-534 @about=REQ-298 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: JSONの外形と版
  Given 本体の既存仕様を適用する
  When rw-copy用not_copiedとcopied-fileを追加したJSONを見る
  Then format_versionは1を維持する

```

```gherkin
@id=EX-535 @about=REQ-299 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: JSONの主要キー
  Given 本体の既存仕様を適用する
  When COMMANDなしでJSON計画を見る
  Then commandはnullでvariablesに4変数を持つ

@id=EX-536 @about=REQ-300 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: JSONの秘密と記述子
  Given 本体の既存仕様を適用する
  When 秘密とrw-copyを含むJSON計画を見る
  Then 秘密とコピー内容は含まず記述子の種類と長さを示す

@id=EX-537 @about=REQ-301 @source=docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: JSONの出所と文字
  Given 本体の既存仕様を適用する
  When secret由来のマウントをJSON表示する
  Then originのkindはsecretでnameを持つ

```

```gherkin
@id=EX-759 @about=REQ-403 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A12
Scenario: 差し替えられるroへの走査のリンク
  Given 走査で一致したリンクの先が、解決がrwの中を通るroの項目の中にある
  When 段階7の生成を行う
  Then リンクを隠さずpathの診断で止まる

@id=EX-760 @about=REQ-403 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A12
Scenario: 読めないマウント一覧
  Given mounts.hide-mountsが書かれていてマウント一覧を読めない
  When 段階7の生成を行う
  Then 隠すマウントを見落としうるのでpathの診断で止まる

@id=EX-761 @about=REQ-404 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A12
Scenario: 上限を超える複製元
  Given rw-copyのディレクトリが4097個のエントリを持つ
  When 段階7のrw-copyの読み込みを行う
  Then 複製を始めずpathの診断で止まる

@id=EX-762 @about=REQ-404 @source=docs/decision/brainstorm/2026-09-25-spec-only-rules.md#A12,docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
Scenario: 名前付きパイプのrw-copy
  Given rw-copyの項目の実体が名前付きパイプである
  When 段階7のrw-copyの読み込みを行う
  Then 内容を待たずpathの診断で止まる

```
