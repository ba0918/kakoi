# Glossary

既存仕様から継承した本体の検査用表現（core-*）を置く。

| Term | Meaning | Source |
|---|---|---|
| ガードレール | 隔離の中で起動されるプログラムの使い方を規則で止める、悪意があれば迂回できる事故防止の柵。境界（カーネルが強制する制限）とは呼ばない | docs/decision/brainstorm/2026-09-25-command-policy.md#A1 |
| 見張り役 | ガードレールの規則を当てるために、規則のあるプログラムの名前で PATH の先頭に置き、規則が求めるときは本物の場所にも重ねる kakoi 自身の実行ファイル | docs/decision/brainstorm/2026-09-25-command-policy.md#A3, docs/decision/brainstorm/2026-09-25-command-policy.md#A26 |
| 入れ子の印 | kakoi が隔離の中の "/dev/kakoi-isolated" に読み取り専用のマウントで置くファイル。入れ子の判定だけに使う | docs/decision/brainstorm/2026-09-29-nested-isolation.md#A23, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A30 |
| 共有ファイルの置き場 | 入れ子でない起動が、ファイルの "hide" に使う空のファイルと filtered の "/etc/resolv.conf" を実在のファイルとして置くホストの "$XDG_RUNTIME_DIR/kakoi/"。隔離の中に見せるかはポリシーに従う | docs/decision/brainstorm/2026-09-29-nested-isolation.md#A16, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A42, docs/decision/brainstorm/2026-09-29-nested-isolation.md#A19 |
| 土台 | マウントのモードが "listed" のときに、"mounts.system" が true なら kakoi がまとめて読み取り専用で見せるホストのディレクトリ "/usr"、"/bin"、"/sbin"、"/lib"、"/lib64"、"/etc"。ソケットや認証情報を置く場所（"/run"、"/tmp"、ホーム、"/mnt"）を含まない | docs/decision/brainstorm/2026-09-30-allowlist.md#A3 |
| 見せた場所 | マウントのモードが "listed" のときに隔離の中に見えるもの。`土台`、"rw"、"rw-file"、"rw-copy"、"ro" の項目、"/tmp"、名前解決の設定の指す先と、kakoi が自分で置くもの（見張り役、置き直した本物、filtered の "/etc/resolv.conf"、"/dev" の印、`隔離の中の最初のプロセス` の実行ファイル） | docs/decision/brainstorm/2026-09-30-allowlist.md#A3, docs/decision/brainstorm/2026-09-30-allowlist.md#A5, docs/decision/brainstorm/2026-09-30-allowlist.md#A8, docs/decision/brainstorm/2026-09-30-allowlist.md#A9, docs/decision/brainstorm/2026-09-30-allowlist.md#A43 |
| 隔離の中の最初のプロセス | コマンドのモードが "listed" のときに bwrap がコマンドの代わりに起動する kakoi 自身の実行ファイル。Landlock を掛けてからコマンドを起動する | docs/decision/brainstorm/2026-09-30-allowlist.md#A41 |
