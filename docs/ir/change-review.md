# 変更照合の運用

Git差分と判断記録の照合、独立レビュー、ローカルとPR CIの比較対象を定める。
製品の動作要求は変更しない。

## Requirements

### REQ-1000: 最終変更照合

- kind: invariant
- source: docs/decision/brainstorm/2026-10-03-kotowari-changes.md#A1, docs/decision/brainstorm/2026-10-03-kotowari-changes.md#A2
- verification: review
- how_to_verify: 設定の対象を確認し、呼出し側がブランチまたはCIイベントから求めた完全な比較元とheadでcheckとchangesのreview phaseを実行する。実装者と独立レビュアーの記録の根拠、要求との整合、委譲範囲を確認する。

統合前には、呼出し側が固定したブランチ全体の比較元と対象headについて、設定対象の変更を実装者と独立レビュアーが各自記録し、kotowari checkとkotowari changesのreview phaseがともに終了0になることを要求する。

### REQ-1001: 記録の寿命と実行場所

- kind: invariant
- source: docs/decision/brainstorm/2026-10-03-kotowari-changes.md#A3, docs/decision/brainstorm/2026-10-03-kotowari-changes.md#A4, docs/decision/brainstorm/2026-10-03-kotowari-changes.md#D1
- verification: review
- how_to_verify: hookにchangesがないこと、PR CIが実際のheadとイベント両端のmerge-baseを使うこと、PROJECT.mdの運用指示と両記録の対象が一致することを確認する。対象内容、IR、判断の意味、比較元が変わった場合には両役が再照合した証拠を確認する。

変更照合はhookでは実行せず、任意のstaged実装者確認と統合前の独立レビューで実行し、現在の比較だけを固定名の両役の記録に保持し、内容または比較元の変更後は両記録を作り直す。
