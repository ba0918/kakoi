# filtered の起動で見つかるポリシーの誤りの診断と、review 要求の確かめ方

## Context

filtered の起動の処理で見つかる誤りは、ポリシーが原因のものも種類 bwrap の診断になる（[REQ-427 の決定](./2026-09-25-spec-only-rules.md#A8)）。
`host-interface` を含む宛先の拒否をポリシーの誤り（種類 policy）に移すかは未決として残した（[U5](./2026-09-25-spec-only-rules.md#A8)）。
また、verification が review の要求 69 件に `how_to_verify` の行が無く、`kotowari check` の誤りとして残っている。
review の要求は、人か LLM がどう確かめるかが書かれていないと、確かめたことにできない。
この 2 つを決める。

Position: IR とガイドを書き終え、照合を 3 回終えた。承認待ち。

## Agreements

- A1 合成後のモードが filtered のとき、`host-interface` を含む宛先の許可はポリシーの合成の段階で拒否し、種類 policy の診断を出して 125 で終わる。合成後のモードが host か none なら、今までどおり使わない設定として形式だけを検査して通す。
  - why: 原因はポリシーの中身だけで、起動の処理に入らなくても分かる。読み込みと合成で見つかる誤りを policy にする規則（REQ-426）と揃う
  - rejected: 起動の処理で種類 bwrap のまま拒否する案。ポリシーを直すべき誤りが bwrap の失敗として報告され、利用者が原因を取り違える
  - decided_by: the user (took the recommendation)
- A2 A1 の拒否は `--print-plan` にも当てはめ、計画を表示せずに同じ policy の診断で終わる。
  - why: 計画を見た段階で起動できないことが分かるほうが利用者にとってよい。計画の表示は起動と同じ検査を通る（REQ-292）
  - rejected: `--print-plan` では通し、起動のときだけ止める案。計画は表示されるのに起動できない食い違いが残る
  - decided_by: the user (took the recommendation)
- A3 ホスト DNS を使うときにホストの名前解決設定に nameserver が無い場合は、今のまま filtered の起動の処理で種類 bwrap の診断を出して 125 で終わる。これはポリシーではなくホストの環境が原因の誤りとして、ポリシーが原因の誤りと書き分ける。
  - why: 原因がホストの環境にあるので、policy にすると利用者がポリシーを直しに行って迷う
  - rejected: policy に移す案、診断の新しい種類を作る案。前者は原因を取り違えさせ、後者は診断の種類の契約（REQ-290）を広げる
  - decided_by: the user (took the recommendation)
- A4 verification が review で `how_to_verify` の無い要求すべてに、確かめ方を書き足す。確かめ方は、要求の本文が名指す成果物（文書、スキル、雛形、用語集）を読み、本文の各条件がそこに書かれているか、そこから観測できるかを確かめる手順として書く。本文に無い条件を確かめ方で足さない。
  - why: 確かめ方の無い review の要求は、確かめたことにできない
  - rejected: 別の作業に回す案。`kotowari check` の誤りが残り続け、IR の品質の判断が曇る
  - decided_by: the user (took the recommendation)

## Delegated

- D1 A1〜A3 を IR のどの要求に書くか（REQ-426、REQ-427 の改め方、新しい要求にするか）、ガイドの書き直し、A4 の各要求の確かめ方の文面は、決定の意味を変えない範囲で Claude が決める。A4 の文面は利用者がまとめて確かめる。
  - why: 置き場所と文面は決定の内容から決まり、A4 は件数が多く下書きを見て判断するほうが速い

## Undecided

なし。

## Revisions

- U1〜U3 は A1〜A4 で決めた。[2026-09-25-spec-only-rules の U5](./2026-09-25-spec-only-rules.md#A8) は A1 と A3 で決まった。
- 照合を 3 回行った。確かめ方が要求の本文より広かった 6 件（REQ-025、REQ-132、REQ-174、REQ-367、REQ-368、REQ-391）の確かめ方を狭め、REQ-391 と REQ-392 に kakoi-net の A155 を出典として足した。REQ-391 の「公開の判断と pasta 固有の操作を分ける」という区切り方は 3 回目でも決定に根拠が見つからず、FLAG-027 に記録した。
- 確かめ方を足したことで core-terms の文書が 223 行になり行数の目安を超えたが、分けない。1 行ずつの確かめ方を足しただけで、2026-09-16 の整理の A5 に従う。
