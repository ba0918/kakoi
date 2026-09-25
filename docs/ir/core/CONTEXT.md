# Glossary

既存仕様から継承した本体の検査用表現（core-*）を置く。

| Term | Meaning | Source |
|---|---|---|
| ガードレール | 隔離の中で起動されるプログラムの使い方を規則で止める、悪意があれば迂回できる事故防止の柵。境界（カーネルが強制する制限）とは呼ばない | docs/decision/brainstorm/2026-09-25-command-policy.md#A1 |
| 見張り役 | ガードレールの規則を当てるために、規則のあるプログラムの名前で PATH の先頭に置き、規則が求めるときは本物の場所にも重ねる kakoi 自身の実行ファイル | docs/decision/brainstorm/2026-09-25-command-policy.md#A3, docs/decision/brainstorm/2026-09-25-command-policy.md#A26 |
