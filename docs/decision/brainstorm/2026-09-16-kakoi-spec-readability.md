# kakoi本体の仕様整理とIR抽出

既存仕様の読みやすさを改善する作業の記録。kakoi-netの追加仕様の判断は既存の記録に残し、ここでは再決定しない。

## Agreements

- A1 本体の既存仕様をIRへ抽出し、それと照合しながら人間向け仕様を書き直す。仕様の正本は docs/spec/kakoi.md を入口とする文書群のまま維持する。今回の抽出は既存の条件・例外・成功条件・対象外の移記であり、新たな製品挙動への合意ではない。継承元はコミット 62436c6e6dea0dbadca4330c5707891d7af39ba3 の docs/spec/kakoi.md 第1〜20節（第7節は従来のhost/noneのみ）と、既に承認されたkakoi-net改訂の接続部分。責務別の対応先は下表に記す。
  - superseded_by: [2026-09-25 の A1](./2026-09-25-kakoi-spec-retirement.md#A1)
- A2 既存テストへの要求IDの対応付けを含める。実際に確認している要求に印を置き、テストの動作は変更しない。
- A3 要求に対応するテストの不在、仕様とテストの矛盾、仕様自身の曖昧さ・矛盾を区別して記録する。テストの不在だけで不具合とは断定しない。文章整理に紛れて製品の挙動を変更しない。
- A5 行数や要求数だけを理由に文書を分割しない。同じ責務は一つにまとめ、異なる責務には内容が分かる名前を付ける。kotowariの件数・行数の警告は、それだけで分割を必要とする判定にはしない。
- A4 実装側で吸収できる文書構成・表現・IDの割当は担当側で決める。新しい設計判断や重要な仕様変更は利用者に確認する。

## Prohibitions

- P1 今回は製品コードとテストの動作を変更しない。既存のkakoi-net要求を本体IRの新しい定義で置き換えない。
- P2 歴史的な不採用理由や未決事項を、新しい確定要求として扱わない。

## Delegated

- D1 見出し、説明順、責務単位の文書分割、図や例の使い方は、意味を変えず参照を維持する条件で担当側が決める。

## Rejected

なし。

## Undecided

抽出・照合で見つかった論点は docs/ir/FLAGS.md に根拠と影響を記録し、ここで決定済みにしない。

## Revisions

従来のkotowari試走範囲はkakoi-netの追加・改訂部分だけだった。A2により本体IRと既存テストの対応付けを今回の範囲に追加する。

## 既存条項の継承先

（2026-09-25 追記）ここが指す docs/spec は削除した（[決定記録](./2026-09-25-kakoi-spec-retirement.md#A1)）。仕様の文面は Git の履歴で、旧節番号とガイドの章の対応は docs/guide/appendix/spec-sections.md で引ける。以下は削除前の記録として残す。

| 旧節 | 人間向け仕様 | 内容 |
|---|---|---|
| 1・3・18 | [目的・範囲](../../spec/kakoi/overview.md) | 実行結果、対応環境、対象外 |
| 2 | [用語](../../spec/kakoi/terms.md) | パス、段、計画の定義 |
| 4 | [CLI](../../spec/kakoi/cli.md) | 文法、init、コマンド解決 |
| 5.1〜5.5 | [ポリシー](../../spec/kakoi/policy.md) | 入力、合成、同一性 |
| 5.6 | [配置保護](../../spec/kakoi/policy-placement.md) | 差し替えへの防御 |
| 6 | [マウント](../../spec/kakoi/mounts.md) | 指令、生成、適用順 |
| 8〜10 | [環境](../../spec/kakoi/environment.md) | 環境変数、秘密、Git |
| 11 | [端末](../../spec/kakoi/terminal.md) | seccompの境界 |
| 12〜13 | [プロセス](../../spec/kakoi/process.md) | 入れ子、出力、診断の順序 |
| 14 | [実行境界](../../spec/kakoi/runtime.md) | I/Oと外部依存 |
| 15 | [検証](../../spec/kakoi/verification.md) | 純粋関数と実バイナリの観測 |
| 16 | [公開文書](../../spec/kakoi/documentation.md)、[シム](../../spec/kakoi/shim.md)、[セットアップ](../../spec/kakoi/setup-skill.md) | 説明する事項と補助物の契約 |
| 17 | [リリース](../../spec/kakoi/release.md) | 版と配布物 |
| 19〜20 | [不採用案](../../spec/kakoi/alternatives.md)、[未決・裁量](../../spec/kakoi/delegation.md) | 履歴と判断の境界 |

## 本体IRの見出し一覧

要求IDはテストとの対応に使う。印の存在は、要求内の全条件を検証した証拠ではない。

| 要求 | 検査用文書 | 内容 |
|---|---|---|
| REQ-151 | [core-policy.md](../../ir/core/core-policy.md) | 固定キー・TOML・必須値 |
| REQ-152 | [core-policy.md](../../ir/core/core-policy.md) | パス記法と Git の相互参照 |
| REQ-153 | [core-policy.md](../../ir/core/core-policy.md) | 変数を展開できない場合と対象外の値 |
| REQ-154 | [core-policy.md](../../ir/core/core-policy.md) | 選択するプロファイルと合成の優先順位 |
| REQ-155 | [core-policy.md](../../ir/core/core-policy.md) | 削除を持たない合成とワイルドカード |
| REQ-156 | [core-policy.md](../../ir/core/core-policy.md) | 実体による同一性と段ごとの競合 |
| REQ-157 | [core-policy.md](../../ir/core/core-policy.md) | 合成後に効かない env.pass の拒否 |
| REQ-158 | [core-policy-placement.md](../../ir/core/core-policy-placement.md) | 保護対象と参照経路の検査 |
| REQ-159 | [core-policy-placement.md](../../ir/core/core-policy-placement.md) | 根への着地と隠し先・起点のリンク |
| REQ-160 | [core-policy-placement.md](../../ir/core/core-policy-placement.md) | 起点のマウント点とワークスペースの例外 |
| REQ-161 | [core-policy-placement.md](../../ir/core/core-policy-placement.md) | 再露出・固定マウント・広すぎる書き込みの拒否 |
| REQ-162 | [core-policy-placement.md](../../ir/core/core-policy-placement.md) | 根の項目と露出する組の根拠 |
| REQ-163 | [core-policy-placement.md](../../ir/core/core-policy-placement.md) | 残る配置上の隙間と受け入れる形 |
| REQ-164 | [core-policy-placement.md](../../ir/core/core-policy-placement.md) | 読み取り専用で重ねる保護の限界 |
| REQ-165 | [core-policy-placement.md](../../ir/core/core-policy-placement.md) | 配置保護の受け入れ例と拒否例 |
| REQ-166 | [core-policy-placement.md](../../ir/core/core-policy-placement.md) | 複製・生成・起点に関する境界例 |
| REQ-167 | [core-mounts.md](../../ir/core/core-mounts.md) | 五つの指令と複製の生成 |
| REQ-168 | [core-mounts.md](../../ir/core/core-mounts.md) | 複製の改名・上限・読み取り失敗 |
| REQ-169 | [core-mounts.md](../../ir/core/core-mounts.md) | 存在しないマウント項目の飛ばし |
| REQ-170 | [core-mounts.md](../../ir/core/core-mounts.md) | 走査・マウント・秘密からの hide 生成 |
| REQ-171 | [core-mounts.md](../../ir/core/core-mounts.md) | 生成の優先順位と検査対象の確定 |
| REQ-172 | [core-mounts.md](../../ir/core/core-mounts.md) | 祖先から子孫へ適用するマウント順 |
| REQ-173 | [core-mounts.md](../../ir/core/core-mounts.md) | 作業場所の警告・拒否と次回起動の隙間 |
| REQ-174 | [core-terms.md](../../ir/core/core-terms.md) | 用語 |
| REQ-175 | [core-terms.md](../../ir/core/core-terms.md) | 隔離 |
| REQ-176 | [core-terms.md](../../ir/core/core-terms.md) | ポリシー |
| REQ-177 | [core-terms.md](../../ir/core/core-terms.md) | ポリシーファイル |
| REQ-178 | [core-terms.md](../../ir/core/core-terms.md) | プロファイル |
| REQ-179 | [core-terms.md](../../ir/core/core-terms.md) | 組み込みの既定 |
| REQ-180 | [core-terms.md](../../ir/core/core-terms.md) | シム |
| REQ-181 | [core-terms.md](../../ir/core/core-terms.md) | モデルが動かない |
| REQ-182 | [core-terms.md](../../ir/core/core-terms.md) | セットアップスキル |
| REQ-183 | [core-terms.md](../../ir/core/core-terms.md) | ホームディレクトリ |
| REQ-184 | [core-terms.md](../../ir/core/core-terms.md) | 設定ディレクトリ |
| REQ-185 | [core-terms.md](../../ir/core/core-terms.md) | 正本 |
| REQ-186 | [core-terms.md](../../ir/core/core-terms.md) | 段 |
| REQ-187 | [core-terms.md](../../ir/core/core-terms.md) | グローバルスコープ |
| REQ-188 | [core-terms.md](../../ir/core/core-terms.md) | プロセススコープ |
| REQ-189 | [core-terms.md](../../ir/core/core-terms.md) | ワークスペース |
| REQ-190 | [core-terms.md](../../ir/core/core-terms.md) | ワークツリー |
| REQ-191 | [core-terms.md](../../ir/core/core-terms.md) | 指令 |
| REQ-192 | [core-terms.md](../../ir/core/core-terms.md) | 走査 |
| REQ-193 | [core-terms.md](../../ir/core/core-terms.md) | 秘密 |
| REQ-194 | [core-terms.md](../../ir/core/core-terms.md) | 計画 |
| REQ-195 | [core-terms.md](../../ir/core/core-terms.md) | 診断 |
| REQ-196 | [core-terms.md](../../ir/core/core-terms.md) | 入れ子 |
| REQ-197 | [core-terms.md](../../ir/core/core-terms.md) | 解決が参照するもの |
| REQ-198 | [core-terms.md](../../ir/core/core-terms.md) | 根の項目 |
| REQ-199 | [core-terms.md](../../ir/core/core-terms.md) | 露出する組 |
| REQ-250 | [core-cli-syntax.md](../../ir/core/core-cli-syntax.md) | 呼び出しの形 |
| REQ-251 | [core-cli-syntax.md](../../ir/core/core-cli-syntax.md) | プロファイル名 |
| REQ-252 | [core-cli-syntax.md](../../ir/core/core-cli-syntax.md) | オプションの重複と値 |
| REQ-253 | [core-cli-syntax.md](../../ir/core/core-cli-syntax.md) | 計画表示の文法 |
| REQ-254 | [core-cli-syntax.md](../../ir/core/core-cli-syntax.md) | CLIのパス |
| REQ-255 | [core-init.md](../../ir/core/core-init.md) | helpとversion |
| REQ-256 | [core-init.md](../../ir/core/core-init.md) | initの作成内容 |
| REQ-257 | [core-init.md](../../ir/core/core-init.md) | initのリンクとモード |
| REQ-258 | [core-init.md](../../ir/core/core-init.md) | initの出力と上書き拒否 |
| REQ-259 | [core-init.md](../../ir/core/core-init.md) | initの独立した検査 |
| REQ-260 | [core-command-resolution.md](../../ir/core/core-command-resolution.md) | コマンド探索 |
| REQ-261 | [core-command-resolution.md](../../ir/core/core-command-resolution.md) | 計画と入れ子のコマンド探索 |
| REQ-262 | [core-command-resolution.md](../../ir/core/core-command-resolution.md) | exec失敗の違い |
| REQ-263 | [core-command-resolution.md](../../ir/core/core-command-resolution.md) | argv0の保持 |
| REQ-264 | [core-command-resolution.md](../../ir/core/core-command-resolution.md) | ホストでの探索の限界 |
| REQ-268 | [core-environment.md](../../ir/core/core-environment.md) | 環境の7段階 |
| REQ-269 | [core-environment.md](../../ir/core/core-environment.md) | 環境の受け渡し |
| REQ-270 | [core-environment.md](../../ir/core/core-environment.md) | PATHの先頭追加 |
| REQ-271 | [core-environment.md](../../ir/core/core-environment.md) | 秘密の置換と欠落 |
| REQ-272 | [core-environment.md](../../ir/core/core-environment.md) | 秘密の末尾改行 |
| REQ-273 | [core-environment.md](../../ir/core/core-environment.md) | 秘密の値の拒否 |
| REQ-274 | [core-environment.md](../../ir/core/core-environment.md) | 秘密を表示しない |
| REQ-275 | [core-environment.md](../../ir/core/core-environment.md) | 継承した認証情報 |
| REQ-276 | [core-environment.md](../../ir/core/core-environment.md) | Git設定の追加 |
| REQ-277 | [core-environment.md](../../ir/core/core-environment.md) | Gitの番号検査 |
| REQ-278 | [core-environment.md](../../ir/core/core-environment.md) | 環境値を使う合成の限定 |
| REQ-280 | [core-terminal.md](../../ir/core/core-terminal.md) | アーキテクチャの検査 |
| REQ-281 | [core-terminal.md](../../ir/core/core-terminal.md) | x32の拒否 |
| REQ-282 | [core-terminal.md](../../ir/core/core-terminal.md) | TIOCSTIの拒否 |
| REQ-283 | [core-terminal.md](../../ir/core/core-terminal.md) | 端末保護の範囲 |
| REQ-284 | [core-process.md](../../ir/core/core-process.md) | 入れ子の実行 |
| REQ-285 | [core-process.md](../../ir/core/core-process.md) | 入れ子の計画 |
| REQ-286 | [core-process.md](../../ir/core/core-process.md) | 入れ子検出の限界 |
| REQ-287 | [core-process.md](../../ir/core/core-process.md) | 並列起動 |
| REQ-288 | [core-process.md](../../ir/core/core-process.md) | 診断と警告の形 |
| REQ-289 | [core-output.md](../../ir/core/core-output.md) | 制御文字の表示 |
| REQ-290 | [core-output.md](../../ir/core/core-output.md) | 診断の終了コード |
| REQ-291 | [core-output.md](../../ir/core/core-output.md) | コマンドとbwrapの終了 |
| REQ-292 | [core-output.md](../../ir/core/core-output.md) | 計画の事前検査 |
| REQ-293 | [core-output.md](../../ir/core/core-output.md) | 検査の段階 |
| REQ-294 | [core-plan-output.md](../../ir/core/core-plan-output.md) | マウント段階内の順序 |
| REQ-295 | [core-plan-output.md](../../ir/core/core-plan-output.md) | 同段階の診断順序 |
| REQ-296 | [core-plan-output.md](../../ir/core/core-plan-output.md) | 要約の内容 |
| REQ-297 | [core-plan-output.md](../../ir/core/core-plan-output.md) | 全量と共通表示 |
| REQ-298 | [core-plan-output.md](../../ir/core/core-plan-output.md) | JSONの外形と版 |
| REQ-299 | [core-plan-output.md](../../ir/core/core-plan-output.md) | JSONの主要キー |
| REQ-300 | [core-plan-output.md](../../ir/core/core-plan-output.md) | JSONの秘密と記述子 |
| REQ-301 | [core-plan-output.md](../../ir/core/core-plan-output.md) | JSONの出所と文字 |
| REQ-305 | [core-runtime.md](../../ir/core/core-runtime.md) | ホストへの書込み境界 |
| REQ-306 | [core-runtime.md](../../ir/core/core-runtime.md) | 収集する事実 |
| REQ-307 | [core-runtime.md](../../ir/core/core-runtime.md) | 外部コマンド |
| REQ-308 | [core-runtime.md](../../ir/core/core-runtime.md) | メモリ上の記述子 |
| REQ-309 | [core-runtime.md](../../ir/core/core-runtime.md) | ファイル数上限 |
| REQ-310 | [core-runtime.md](../../ir/core/core-runtime.md) | 通常ファイルだけを読む |
| REQ-311 | [core-runtime.md](../../ir/core/core-runtime.md) | 読込量の上限 |
| REQ-312 | [core-runtime.md](../../ir/core/core-runtime.md) | 読むリンクの区別 |
| REQ-313 | [core-runtime.md](../../ir/core/core-runtime.md) | パス解決のリンク数 |
| REQ-314 | [core-runtime.md](../../ir/core/core-runtime.md) | 信頼する起動環境 |
| REQ-315 | [core-runtime.md](../../ir/core/core-runtime.md) | 固定引数の順序 |
| REQ-316 | [core-runtime.md](../../ir/core/core-runtime.md) | COMMANDなしの固定引数 |
| REQ-350 | [core-product.md](../../ir/core/core-product.md) | 実行結果と引数の保存 |
| REQ-351 | [core-product.md](../../ir/core/core-product.md) | 計画の決定性 |
| REQ-352 | [core-product.md](../../ir/core/core-product.md) | 対応環境 |
| REQ-353 | [core-product.md](../../ir/core/core-product.md) | 対象外の機能 |
| REQ-354 | [core-verification.md](../../ir/core/core-verification.md) | 計画算出の純粋な境界 |
| REQ-355 | [core-verification.md](../../ir/core/core-verification.md) | 純粋関数の検証範囲 |
| REQ-356 | [core-verification.md](../../ir/core/core-verification.md) | 実バイナリの検証範囲 |
| REQ-357 | [core-verification.md](../../ir/core/core-verification.md) | 移行時の一度限りの確認 |
| REQ-358 | [core-public-docs.md](../../ir/core/core-public-docs.md) | 公開文書の導線 |
| REQ-359 | [core-public-docs.md](../../ir/core/core-public-docs.md) | 既知の隙間の公開 |
| REQ-360 | [core-public-docs.md](../../ir/core/core-public-docs.md) | 配置保護の説明 |
| REQ-361 | [core-public-docs.md](../../ir/core/core-public-docs.md) | rw-copyと一時領域の説明 |
| REQ-362 | [core-public-docs.md](../../ir/core/core-public-docs.md) | インストールの説明順 |
| REQ-363 | [core-public-docs.md](../../ir/core/core-public-docs.md) | スキルの隔離外実行の案内 |
| REQ-364 | [core-bundled-profile.md](../../ir/core/core-bundled-profile.md) | 同梱プロファイルの配置と内容 |
| REQ-365 | [core-bundled-profile.md](../../ir/core/core-bundled-profile.md) | 同梱の認証情報と表示経路の除去 |
| REQ-366 | [core-shim.md](../../ir/core/core-shim.md) | 汎用の本体と対象ごとの設定 |
| REQ-367 | [core-shim.md](../../ir/core/core-shim.md) | 既定の隔離と承認済み例外 |
| REQ-368 | [core-shim.md](../../ir/core/core-shim.md) | 一覧の照合範囲と出荷値 |
| REQ-369 | [core-shim.md](../../ir/core/core-shim.md) | 壊れ方に応じた案内 |
| REQ-370 | [core-shim.md](../../ir/core/core-shim.md) | 実体の探索 |
| REQ-371 | [core-shim.md](../../ir/core/core-shim.md) | 共有ディレクトリの作成 |
| REQ-372 | [core-shim.md](../../ir/core/core-shim.md) | フラグと引数の写し |
| REQ-373 | [core-shim.md](../../ir/core/core-shim.md) | シムの人による検証 |
| REQ-374 | [core-setup-skill.md](../../ir/core/core-setup-skill.md) | スキルの配置と配布 |
| REQ-375 | [core-setup-skill.md](../../ir/core/core-setup-skill.md) | 正本と未作成プロファイルの確認 |
| REQ-376 | [core-setup-skill.md](../../ir/core/core-setup-skill.md) | 対象に合わせた提案 |
| REQ-377 | [core-setup-skill.md](../../ir/core/core-setup-skill.md) | 引数写しの制限の案内 |
| REQ-378 | [core-setup-skill.md](../../ir/core/core-setup-skill.md) | 差分を示してから書く |
| REQ-379 | [core-setup-skill.md](../../ir/core/core-setup-skill.md) | 書き先と別の正本の境界 |
| REQ-380 | [core-setup-skill.md](../../ir/core/core-setup-skill.md) | 読めない設定の扱い |
| REQ-381 | [core-setup-skill.md](../../ir/core/core-setup-skill.md) | 秘密への非アクセス |
| REQ-382 | [core-setup-skill.md](../../ir/core/core-setup-skill.md) | 実行可能ビットと経路の確認 |
| REQ-383 | [core-setup-skill.md](../../ir/core/core-setup-skill.md) | PATHの不一致と隔離内PATHの区別 |
| REQ-384 | [core-setup-skill.md](../../ir/core/core-setup-skill.md) | 計画で反映を確認 |
| REQ-385 | [core-setup-skill.md](../../ir/core/core-setup-skill.md) | スキルの人による検証 |
| REQ-386 | [core-release.md](../../ir/core/core-release.md) | 版の唯一の典拠 |
| REQ-387 | [core-release.md](../../ir/core/core-release.md) | 配布物の契約 |
| REQ-388 | [core-network-existing.md](../../ir/core/core-network-existing.md) | 従来のhostとnoneの通信範囲 |

## 照合の結果

既存304テストはすべて成功。302テストに要求IDを付け、コメントを取り除くと全テストが変更前とバイト単位で一致した。製品コードは変更していない。

本体IRは148要求。要求IDの印がない4要求は次のとおり。印が付いた要求にも、条件の一部が未検証のものがある。

| 要求 | 未確認の内容 |
|---|---|
| REQ-283 | 通常の端末ioctlの許容と制御端末の維持 |
| REQ-286 | 環境を空にした入れ子と、それによる秘密読取失敗の境界 |
| REQ-313 | 40本と41本のリンクの境界、および対象別の扱い |
| REQ-314 | 起動環境を信頼するという境界全体。個別の環境依存動作のテストとは区別する |

印のない2テストは、1秒という時間上限と非UTF-8マウント名の扱いを確認するもの。今回新たな仕様として追認せず、FLAG-007に記録した。

具体的な仕様矛盾・曖昧さ・部分的テスト不足は [FLAGS](../../ir/FLAGS.md) に10件記録した。新しい製品挙動への判断はしていない。

独立レビューは文書の品質と仕様への適合の2観点で実施し、今回生じた条件の欠落・意味変化を修正して再確認した。ただし全テストの全assertと全具体例の個別精査は完了していない。要求IDの対応付けを、完全な網羅検証の証明には使わない。

行数・件数の警告はA5に従って保持する。構文・出典参照・IDの整合エラーとは区別し、警告の解消だけを目的に分割しない。既存kakoi-netの未実装要求のテスト不足も、本体の不足とは別に残る。

## 承認と次の作業

2026-09-16、kemiで55ファイルの提示内容が承認された。承認時に全ファイルのSHA-256が提示時と一致することを確認した。

利用者コメント：仕様とテストを機械的に紐づけるIRへ抽出できた点を評価。一方、人間向け仕様の文章品質は十分ではない。追加の文書校正は別の機会へ回し、本丸のkakoi-net実装を優先する。

本体の製品挙動を変えず、FLAGSの問題は未解決として残す。次工程は既存の合意どおり、対応環境で実証の完了条件を満たしてから製品実装へ進む。承認後の追記は、この承認記録と文書の状態表示の更新に限る。
