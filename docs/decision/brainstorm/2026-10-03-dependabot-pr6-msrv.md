# Dependabot PR6のMSRV検査修正

## Context

Dependabot PR6はMSRVジョブのRust指定を1.88.0から1.120.0に変更し、未提供版のインストールで失敗した。
利用者の依頼はPRの確認と取り込みであり、MSRV変更の承認ではない。
呼出し側は既存の1.88を維持し、アクション実装だけを更新できる修正を実装担当へ委譲した。
Cargo.tomlとPROJECT.mdの最低対応版、既存のビルド検査と統合ゲートは変更しない。

## Agreements

- A1 MSRVステップのみ、入力を受け取るdtolnay/rust-toolchainの公式master実装を完全SHAで固定し、with.toolchainを1.88.0と明示する。
  - why: 版付きrefはRust版をハードコードするため、アクション更新と最低対応版の選択を分離する。公式masterは7e38f4b43b4db5c8dd498af069a4f6196df1d067に解決し、そのaction.ymlはtoolchain入力をインストール対象へ渡す。既存のDependabot github-actions設定はアクション参照を更新でき、Rust指定は別入力として残る。上流の[commit](https://github.com/dtolnay/rust-toolchain/commit/7e38f4b43b4db5c8dd498af069a4f6196df1d067)、[action.yml](https://github.com/dtolnay/rust-toolchain/blob/7e38f4b43b4db5c8dd498af069a4f6196df1d067/action.yml)と[LICENSE](https://github.com/dtolnay/rust-toolchain/blob/7e38f4b43b4db5c8dd498af069a4f6196df1d067/LICENSE)を読み取りで確認し、ソースは転記しない。
  - decided_by: 実装担当。呼出し側が委譲した技術的修正の範囲で選択。利用者がこのSHAや方式を直接指定したとは記録しない。

## Delegated

- D1 実装担当は修正と現在の比較の実装者記録を作成する。独立レビューとレビュアー記録、コミット、push、PR操作、統合は呼出し側に残す。
  - why: [REQ-1000](../../ir/change-review.md#REQ-1000)の独立レビューと[REQ-1001](../../ir/change-review.md#REQ-1001)の記録更新を守り、実装者による自己承認を避けるため。
  - decided_by: 呼出し側の今回の委譲。
