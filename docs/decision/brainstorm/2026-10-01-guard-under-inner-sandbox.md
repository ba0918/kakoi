# 中で sandbox を作るツールと見張り役

## Context

kakoi の中で codex を動かすと、git の規則に "guard-absolute-path" を付けたプロファイルで `git --version` が `kakoi 0.5.0` を返し、git が使えなかった（利用者の報告、2026-10-01）。
codex は自分の sandbox で `/dev` を作り直すので、見張り役の表と置き直した本物が見えなくなり、`/usr/bin/git` に重ねた kakoi が kakoi 自身として動いた。
同じ調べで、gh の規則の "guard-absolute-path" が、gh の本物をたどった先の mise の実行ファイルそのものに見張り役を重ね、mise を通して起動するほかのツールにも gh の規則が当たる形になっていることが分かった。
利用者は、見張り役が黙って別物にならないことと、別の名前の実行ファイルに重ねないことを選んだ。

事実（2026-10-01、この環境、main ec6d512 の kakoi、一時的な HOME）: git の規則に "guard-absolute-path = true" を付けた隔離の中で、`git --version` は "git version 2.43.0" を返し、中で `bwrap --ro-bind / / --dev /dev --proc /proc` を通すと `kakoi 0.5.0` を返し、`git status` は `kakoi: usage: invalid command line` で終わった。
事実（同日、利用者の codex のセッション）: 隔離の最初のプロセスの引数に、`--ro-bind ~/.cargo/bin/kakoi ~/.local/bin/mise` と `--ro-bind ~/.local/bin/mise /dev/kakoi-guard/real/1/mise` があった。`~/.local/share/mise/shims/gh` は `~/.local/bin/mise` へのリンクである。
事実（同日、利用者）: codex 0.159.2 は、起動引数に `--dangerously-bypass-approvals-and-sandbox` を付けても、設定ファイルに書いた `sandbox_mode` の sandbox でコマンドを動かした。設定ファイルを `sandbox_mode = "danger-full-access"` にすると git が動いた。コマンドを動かしていたのは常駐する `codex app-server --managed-daemon` の子だった。

Position: A1〜A4 を IR（REQ-446、REQ-450、REQ-453、TBL-160、REQ-359、EX-658、EX-964、EX-965）に写し、照合を 2 回行って指摘 0 件。承認待ち。

## Agreements

- A1 規則が "guard-absolute-path" を真にしていても、本物のリンクを解決した実体のファイル名がそのプログラムの名前と違うときは、実体に見張り役を重ねず、本物を置き直さない。見張り役の場所の見張り役は置く。重ねなかったことと理由を "--print-plan" の要約と全量と JSON に示す。
  - why: mise や busybox のように、一つの実行ファイルが起動された名前で別のプログラムとして動くものに重ねると、そのプログラムの規則が、その実行ファイルを通して起動するほかのすべてのプログラムに当たる。
  - rejected: 今のまま、同じ本物を指す名前にすべての規則を当て、既知の隙間に書くだけにする案。
  - decided_by: 利用者（推奨を採用）
- A2 起動された実行ファイルのファイル名が "kakoi" でなく、見張り役の場所とプログラムの対応を読めないときは、kakoi として動かず、コマンドを実行せずに種類 "guard" の診断で 126 とする。説明には対応の置き場を読めないことと、`/dev` を作り直す sandbox の中ではこの見張り役を使えないことを含める。ファイル名が "kakoi" なら、今までどおり見張り役として動かない。
  - why: 対応を読めないときに kakoi として動くと、`git --version` が kakoi の版を返し、`git status` が kakoi の使い方の誤りになるなど、原因の分からない壊れ方をする。見張り役として重ねた場所から起動された kakoi は、その場所のファイル名（`git`）で起動される。
  - rejected: 今のまま kakoi として動く案。
  - decided_by: 利用者（推奨を採用）
- A3 A2 の組み合わせ（"guard-absolute-path" の見張り役と、`/dev` を作り直す sandbox を中で作るツール）で、そのプログラムを使えないことを既知の隙間に加え、中で sandbox を作るツールを使うならその sandbox を切るか "guard-absolute-path" を外す、と案内する。
  - why: A2 は壊れ方を分かるようにするだけで、使えるようにはしない。
  - decided_by: 利用者（推奨を採用。「A と一緒に B」の提案）
- A4 codex のシムの見本は変えず、codex では起動引数の `--dangerously-bypass-approvals-and-sandbox` が設定ファイルの `sandbox_mode` を上書きしなかったことと、そのときは設定ファイルで `sandbox_mode = "danger-full-access"` にすることを、シムの説明の文書に書く。
  - why: シムが起動引数で渡せるものでは直らない（利用者の実測）。書くのは codex の一つの版での観測であり、シムの雛形の仕組みは変えない。
  - decided_by: LLM（利用者の実測の整理）
