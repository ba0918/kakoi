# 本体の用語と定義

既存仕様から継承した本体の検査用表現。同じ責務を一文書にまとめ、見出しと要求IDで参照する。この IR が正本である。

## Requirements

### REQ-174: 用語
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

用語の読みと禁止語は "CONTEXT.md" に記録する。本節が意味の典拠である。

### REQ-175: 隔離
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

プロセスを実行するとき、利用者が安全を確保できるかどうかの境界。0.3 では 4 つの次元を持つ。ファイルシステム（見える範囲と書ける範囲）、ネットワーク（ホストと共有するか、切るか、filteredで許可範囲を制限するか）、環境変数（ホストから何を引き継ぎ、何を足すか）、認証情報（隠すファイルと注入する秘密）。

プロセス ID、IPC、UTS、ユーザーの各名前空間は必ず切る。ユーザー名前空間を作れない環境では起動が拒否される。cgroup の名前空間はカーネルが対応しない場合には切らない。いずれも次元として選べない。認証情報の次元が扱うのはファイルと環境変数だけである。UNIX ソケットを通した認証の転送（ssh-agent、gpg-agent、D-Bus 等）は、そのソケットのパスが隔離の中から見えれば、環境変数を消してもパスを直接指定して使える。

ソケットの経路を止めるのは "hide" であり、"env.unset" は補助にすぎない。同梱プロファイル（第 16 節）はこの両方を行う。

### REQ-176: ポリシー
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

"kakoi" が bwrap の引数列と環境、およびfilteredの通信許可を組み立てるのに使う指定の内容。段の合成（第 5 節）を終えた後のもの。

### REQ-177: ポリシーファイル
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

ポリシーを書いた TOML ファイル一般。プロファイルもポリシーファイルの一種。

### REQ-178: プロファイル
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

グローバルスコープにある名前付きのポリシーファイル。"--profile NAME" で名前を指定する。置き場所は設定ディレクトリの "profile/NAME.toml"。

### REQ-179: 組み込みの既定
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

"kakoi" のバイナリに埋め込まれた、同梱プロファイル "examples/profile/default.toml" と同じ内容。"--profile" が "default"（省略時を含む）で "profile/default.toml" が無いときにだけ、その代わりとしてグローバルスコープになる（第 5.3 節）。"kakoi init" がファイルとして書き出す（第 4.1 節）。

### REQ-180: シム
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

利用者が "PATH" に置く薄いスクリプトで、包む対象のコマンドを既定で "kakoi" に通し、対象に固有の差（自前のサンドボックスを切るフラグ、引数の写し、利用者が承認した素通しの例外）を閉じ込めるもの。製品の一部ではなく、雛形を同梱する（第 16 節）。

### REQ-181: モデルが動かない
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

シムの許可リスト（素通し）に入れる前に利用者が確かめる性質。そのサブコマンドの実行中に、包む対象がモデルに問い合わせず、モデルの出力でコマンドを実行する機能を持たないこと（第 16 節）。

### REQ-182: セットアップスキル
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

同梱する SKILL.md で、エージェント CLI に読ませて、利用者の環境に固有の設定（プロファイルへの追加、シムの設置、代替コマンドの "path-prepend"、隔離の中で壊れたコマンドの扱い）を提案させるもの（第 16 節）。

### REQ-183: ホームディレクトリ
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

ホストの環境変数 "HOME" の実体のパス。"HOME" が無い、空、絶対パスでない、実体のパスを得られない（存在しない、途中が辿れない）、または実体がディレクトリでないときは、ポリシーが "~" を使うかにかかわらず、第 13 節の段階 4 で種類"env" の診断で終わる（"--help"、"--version"、"--print-plan" の無い入れ子は例外で、この段階に至らない）。

### REQ-184: 設定ディレクトリ
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

"$XDG_CONFIG_HOME/kakoi/"、"XDG_CONFIG_HOME" が未設定なら "~/.config/kakoi/"。空文字と絶対パスでない値は未設定と同じに扱う（XDG Base Directory の慣習）。成功の観測条件: "XDG_CONFIG_HOME"を相対パスにして起動すると "~/.config/kakoi/" のプロファイルが読まれる。

反例: カレントディレクトリの下の "kakoi/profile/default.toml" が読まれる。プロファイルは "profile/"、秘密ファイルは "secrets/" に置く。"secrets/" は常に隠される（第 6.3 節）。

### REQ-185: 正本
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

利用者が編集する元のファイル。設定ディレクトリの中身やシムの置き場が、そこから生成や同期で作り直されるか、そこへのリンクであるとき、その元を指す。正本が他所にある場所への書き込みは、次の生成で消えるか、正本を書き換える。設定ディレクトリの中に置かれ、他から作り直されないファイルは、それ自身が正本である。

### REQ-186: 段
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

ポリシーの項目に優先順位を与える 4 つの層。下から順に、グローバルスコープ、プロセススコープ、コマンドライン（"--rw" と "--hide"）、生成（第 6.3 節で "kakoi" が生成する項目）。下 3 つが書かれた段、最上位が生成された段。上の段が下の段に足すか、上書きする。

### REQ-187: グローバルスコープ
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

一番下の段。"--profile" で選んだプロファイルが与える。

### REQ-188: プロセススコープ
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

下から 2 番目の段。"--policy-file" で指定したポリシーファイルが与える。

### REQ-189: ワークスペース
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

変数 "${workspace}" の値であり、ワークツリーを導出する起点となるディレクトリ。"--workspace PATH" で指定し、省略時はカレントディレクトリ。ワークスペースそのものはマウントされない。マウントされるかはポリシーが変数で指定するかで決まる。

### REQ-190: ワークツリー
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

ワークスペースから親へ向かって最初に ".git" を持つディレクトリ。".git" はシンボリックリンクを辿らずに見て、ディレクトリまたは通常ファイルであるものだけを数える。無ければワークスペース自身。

### REQ-191: 指令
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

マウント表の 1 行の種類。"rw"、"rw-file"、"rw-copy"、"ro"、"hide" の 5 つ。

### REQ-192: 走査
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

"mounts.scan" の指定に従い、起点以下のファイルを名前で探して "hide" の項目を生成すること。

### REQ-193: 秘密
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

"secrets" に書かれた環境変数名とファイルの組、およびファイルから読んで環境変数に入れる値。

### REQ-194: 計画
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

ポリシーと収集した事実から算出した、bwrap に渡す引数列（ファイル記述子の位置は記号）、環境、各マウント項目の適用結果、解決したコマンドのパスの組。filteredでは起動時に確定する通信ポリシーも含む。"--print-plan" はこれを表示する。

### REQ-195: 診断
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

"kakoi" 自身の失敗を伝える標準エラーへの 1 行（第 13 節）。

### REQ-196: 入れ子
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

環境変数 "KAKOI" が "1" の状態で "kakoi" が起動されること。既に隔離の中で動いているプロセスが "kakoi" を起動した場合がこれに当たる。

### REQ-197: 解決が参照するもの
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

あるパスを実体へ解決する途中で、成分を探すために中身を見るディレクトリの実体（".." で戻る前に通ったものを含む）と、辿るシンボリックリンクの置き場（親ディレクトリの実体とリンクの名前）。そのパス自身の実体は、中身を見ないので含まない。解決の結果を変えるにはこのどれかを書き換えるしかない。第 5.6 節で使う。

### REQ-198: 根の項目
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

書き込める項目（合成後の "rw" または "rw-file" の項目。その中とは同一か子孫）のうち、自身の解決が書き込める項目の中（自身の中を含む）にあるものを一切参照しないもの。隔離の中からの書き換えで解決の結果を変えられないので、第 5.6 節の信頼の起点になる。"rw-copy" の項目は書き込める項目に数えない。その中への書き込みはホストに届かず（第 6.1 節）、次の起動が読む内容も、パスの解決の結果も変えられないためである。

### REQ-199: 露出する組
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-09-16-kakoi-spec-readability.md#A1
- verification: review

書き込める項目の中を参照して解決される項目と、それが無効にする "hide"・"ro"・"rw-copy" の項目の組のうち、新しく見えるものや書けるものが生まれるもの。項目が "rw"・"rw-file" で相手が"hide"・"ro"・"rw-copy" のとき、および項目が "ro"・"rw-copy" で相手が "hide" のとき。

相手には書かれた項目のほか、祖先として効いている生成された "hide"（第 6.3 節）も含む。"hide" の項目は何も露出しないので、どの相手とも組にならない。"rw-copy" が組になる向きは 2 つある。相手として無効にされる側では、書き込みがホストに出ない場所を "rw"・"rw-file" が上書きして書き込みを通す。

項目の側では、"ro" と同じく "hide" を無効にしてホストの内容を見せる（"ro" に重なっても、同じ内容を見せて書き戻さないので新しく見えるものは無い）。第 5.6 節で使う。手順の順番を指すときは「段」ではなく「段階」を使う。成功の観測条件: 仕様書と "CONTEXT.md" の中で、これらの語が本節の意味以外で使われていない。

反例: 「設定」や「config」がポリシーの意味で使われている。
