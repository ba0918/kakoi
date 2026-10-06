# 変更照合の廃止

## Context

このリポジトリは[変更照合の導入](./2026-10-03-kotowari-changes.md)で、Git差分を実装者と独立レビュアーの記録と照合する`kotowari changes`を運用してきた。
kotowari 0.5.0は`kotowari changes`と変更の記録を削除し、設定の`changes:`節があるとkotowariが止まるようになった。
kotowari側は、変更の記録が手順を踏んだことを示すだけで仕様との一致を示さないと判断し、コードとIRの一致はkotowariの工程の中でコードをIRと読み合わせて確かめる形に改めた。
利用者は、このリポジトリから変更照合を外し、その運用を定めたIR文書`docs/ir/change-review.md`を要求ごと削除することを承認した。

## Agreements

- A1 変更照合の運用をやめる。変更照合の運用を定めたIR文書`docs/ir/change-review.md`をREQ-1000とREQ-1001ごと削除し、[REQ-site-103](../../ir/site/site-publication.md#REQ-site-103)からサイトを変更照合の対象とする文と確かめ方を除く。あわせて設定の`changes:`節、`.kotowari/changes/`の記録、それを探索から隠す`.ignore`、CIの`kotowari changes`とそのためだけの比較元の計算、PROJECT.mdの変更照合の手順を削除する。IR、`kotowari check`、独立レビューは維持する。
  - why: kotowari 0.5.0が`kotowari changes`と変更の記録を削除し、その前提である「変更の記録で仕様との一致を示せる」という考えも[kotowariの判断の記録](https://github.com/ba0918/kotowari/blob/main/docs/decision/records/2026-10-06-changes-rethink.md)（A4、A10、A21）で退けたため。変更の記録は手順を踏んだことしか示さず、コードとIRの一致はkotowariの工程の中でコードをIRと読み合わせて確かめる。`changes:`節を残すと0.5.0以降のkotowariが止まる。
  - decided_by: 利用者
