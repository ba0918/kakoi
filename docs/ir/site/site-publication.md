# LPと公開経路

承認されたLPの表示、既存文書への導線、GitHub Pagesへの公開対象を定める。本体の隔離動作は変更しない。

## Requirements

### REQ-site-101: LPの表示と導線
- kind: ubiquitous
- source: docs/decision/brainstorm/2026-10-04-landing-page.md#A1
- verification: review
- how_to_verify: ブラウザで英語の初期表示、日本語切替、再読込後の選択保持、1440・390・320pxでの表示、CLI主導線とRust副導線と外部リンクを確認する。保持はブラウザが保存を許可する場合に検証する。

LPは英語を初期表示とし、日本語へ切り替えられる。ブラウザが保存を許可するとき、選択を再読込後も保持する。PCとスマホで本文と図を読め、CLI導入を主導線、Rust組み込みを副導線とする。Rust APIの実験的状態、path依存、crates.io未配布、日本語ガイドへのリンクを明示する。host通信が既定であり指定外ファイルも基本的に読み取り専用で見えること、ホストカーネル共有、資源制限なし、悪意あるプログラムを安全化する保証なし、command guardsの限界を説明する。

### REQ-site-102: 公開範囲と保護
- kind: invariant
- source: docs/decision/brainstorm/2026-10-04-landing-page.md#A2
- verification: review
- how_to_verify: workflowの起動条件、権限、環境、artifactの内容と対象commitのPages結果を確認する。公開URLでページと相対参照のCSS・JavaScriptが取得できることを確認する。

Pagesはmain上のサイト、公開workflow、Cargo.tomlの変更とmain上の手動実行で公開する。公開物はサイトのHTML・CSS・JavaScriptだけとし、文書ツリーや作業ファイルを含めない。既存のgithub-pages環境を利用し、環境保護とセキュリティ設定を迂回しない。

### REQ-site-103: 版と公開対象の同一性
- kind: invariant
- source: docs/decision/brainstorm/2026-10-04-landing-page.md#A3, docs/decision/brainstorm/2026-10-06-drop-change-conformance.md#A1
- verification: review
- how_to_verify: 生成された版表示をCargo.tomlと比べ、公開HTMLのcommitメタデータ、Pages runのheadとremote commitを比較する。

LPの版表示はCargo.tomlから生成する。公開HTMLに対象commitを記録する。

### REQ-site-104: 同一originでの言語選択
- kind: invariant
- source: docs/decision/brainstorm/2026-10-04-landing-page-maintenance.md#A1
- verification: review
- how_to_verify: 共通値と旧値の優先順、無効値の無視、初回読み取りで保存しないこと、明示切替時の保存、保存拒否時の切替、および同一originのTOPとLPの遷移・再読込を確認する。

有効な共通キー "ba0918-language" の "en" または "ja" を優先する。共通値が無効または存在しなければ当該LPの旧キー "kakoi-lp-language" の有効値を読み、それもなければ英語を表示する。明示切替時だけ共通値を保存し、旧値の自動昇格や旧キー同士の新旧推測はしない。保存拒否時もページ内切替を動作させる。選択の引継ぎは次の遷移または再読込で行い、既存タブの即時同期は要求しない。
