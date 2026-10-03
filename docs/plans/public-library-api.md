# Plan: 公開Rust APIと責務別クレートへの移行

## Goal

外部Rustパッケージが公開APIで既存の隔離機能を起動・監督・停止でき、従来のCLIも同じ判断処理を使って従来どおり動く状態にする。

## Specification

仕様の正本は`docs/ir/`である。
設計承認後のコミット`d2a993e74e3706209a552872b94d221870e8914b`を仕様の基準とする。
そのコミットの文書に残る「草案」「承認前」は執筆時の状態表示であり、当該内容は利用者が承認しコミット済みである。
実装済みという意味ではない。
シグナル設定の保証は、その後に利用者が承認した[A28](../decision/brainstorm/2026-10-03-public-library-api.md#A28)による[REQ-library-201](../ir/library/library-lifecycle.md#REQ-library-201)の明確化を適用する。

- [入力](../ir/library/library-input.md): REQ-library-101、REQ-library-102、REQ-library-103、REQ-library-104、REQ-library-105、REQ-library-106。
- [寿命](../ir/library/library-lifecycle.md): REQ-library-201、REQ-library-202、REQ-library-203、REQ-library-204、REQ-library-205、REQ-library-206。
- [観測](../ir/library/library-observation.md): REQ-library-301、REQ-library-302、REQ-library-303、REQ-library-304。
- [組み込み](../ir/library/library-integration.md): REQ-library-401、REQ-library-402、REQ-library-403、REQ-library-404、REQ-library-406、REQ-library-407、REQ-library-408、REQ-library-409、REQ-library-410。
- [責務分割](../ir/network/network-library-boundary.md#REQ-150): REQ-150。[ガード](../ir/core/core-command-guard-runtime.md): REQ-446、REQ-449、REQ-453、REQ-454。[listed](../ir/core/core-listed-commands.md#REQ-475): REQ-475。
- [実行時I/O](../ir/core/core-runtime.md): REQ-305、REQ-306、REQ-307、REQ-308、REQ-309、REQ-315。[終了結果](../ir/core/core-output.md): REQ-291、REQ-401。[入れ子と出力](../ir/core/core-process.md): REQ-284、REQ-285、REQ-288。
- [設定](../ir/core/core-policy.md): REQ-154、REQ-399。[マウント](../ir/core/core-listed-mounts.md#REQ-467): REQ-467。[対応環境](../ir/core/core-product.md#REQ-352): REQ-352。
- [filtered監督](../ir/network/process/process-filtered-supervision.md#REQ-148): REQ-148。[通知](../ir/network/notification/network-notification.md#REQ-068): REQ-068。

型・操作・失敗の表現は[公開API設計契約](../decision/brainstorm/2026-10-03-public-library-api-contract.md)、依存と資源所有は[内部設計](../decision/brainstorm/2026-10-03-public-library-api-design.md)、選択理由は[合意記録](../decision/brainstorm/2026-10-03-public-library-api.md)に従う。
これらを別のAPIへ読み替えず、各ステップの着手時に該当節と`kotowari query ID`を読む。

## Approach and why

純粋なポリシー処理を先に取り出し、次にOS観測と計画の境界を分ける。
既存CLIを動かせる中間状態を保ち、公開APIは外部パッケージから順次試す。
コピー配置、マウントの同一性、子孫の回収を実行試験で確認してからfilteredと非同期観測を接続し、最後に旧クレートへの依存を外す。
局所プローブのファイルは成果物の前提にしない。必要な確認は製品コードを通る試験としてリポジトリに残す。

移管元は`crates/kakoi-core/src/`のpolicy・network・guard・layersの純粋部分、planと周辺の判断、各facts収集とlaunch・Landlock・seccompである。
`layers.rs`は合成とファイル読込み、`shared_files.rs`は型・引数とFS操作、`mount_list.rs`は解析と読込みを分ける。
`planning.rs`は名前に反して観測を伴う手順なのでruntimeへ接続するが、netのfilteredも呼ぶ`locate_command`は下位のlinuxへ分離する。
netの`application.rs`が行うresolver観測とPlan引数の書換えも接続対象とし、runtimeへの逆依存を作らない。
generatorを選んでから観測し、採用されたrw-copyだけを読み、最初の診断で止まる既存の順序を保つ。
すべてのfactsを先に集める変更は挙動不変の抽出とは扱わない。

| 層 | 採用するものと理由 |
|---|---|
| 入力、合成、ガード照合 | 既存coreの型・検証・merge・照合を分離して再利用し、Rust入力とTOMLの別実装を作らない |
| 計画とOS操作 | 既存の計画・観測・bwrap・Landlockを責務別に移し、意味を変える新しい計画器を作らない |
| 通信監督 | 既存netの判断・監督を再利用し、CLI表示を外側へ出す |
| 所有権と待機 | 標準のOwnedFd、Child、Mutex、Condvar、Future、Wakerを使用し、公開APIに非同期ランタイムを必須にしない |
| 内部転送 | 既存serde、serde_json、libcとUnix socketの実装を再利用する。通信契約は設計契約の該当節で固定済み |
| ガードとinitの役割 | 承認済みの役割付きコピー方式を製品へ接続する。共有方式の実装は不要 |
| 外部コンパイル検証 | Cargoの独立パッケージとコンパイラの成功・失敗を利用する。診断の全文や内部型名を固定する専用検査器は作らない |
| 非同期I/Oの実例 | TokioのAsyncFdを利用側だけで使う。所有権付きFDを既存ランタイムへ接続する例であり、runtimeの通常依存には加えない |

計画承認後の実装・修正担当は`openai/gpt-6.1-sol`とする。
このセッションはオーケストレータとなり、残り全ステップを一つの委譲で渡し、実装報告の後に別コンテキストで全差分をレビューする。
指摘を判断してSolへ修正を委譲し、修正差分のレビューを繰り返す。
修正の影響が当該箇所を越える場合だけ、その理由を記録して2回目の全差分レビューを行う。
途中のステップ承認は求めず、停止条件に当たらない限り最終検証と結果報告まで自走する。

## Scope of change

- `Cargo.toml`、`Cargo.lock`と`crates/`。既存core・netの分離、新しいpolicy・plan・linux・runtime、各manifest、直接関係するテストに限る。
- `src/`のCLI接続、診断表示、既存ガード・first-processの接続。コマンド形式と表示の意味は変えない。
- `tests/`の既存回帰試験のimport移行と新しい公開API試験。新しい入口は`tests/library_policy.rs`と`tests/library_api.rs`、細分化は`tests/library_api/`、コンパイル拒否用入力は`tests/fixtures/library-api/`に置く。
- `examples/library-policy/`、`examples/library-sync/`、`examples/library-async/`。各々に独立したCargoパッケージ、lockfile、実行手順を置く。既存の設定例は維持する。
- `.kotowari/config.yaml`は、試験を新クレート内へ移して既存globから外れる場合の試験探索先の追加に限る。`.github/workflows/`は追加例の検証とクレート移動に必要な参照の修正に限る。
- `README.md`、`PROJECT.md`、`docs/guide/`の実装済み利用方法と責務の案内。新規ガイドの入口は`docs/guide/maintainer/library-api.md`とする。
- 実装証拠とレビュー記録は無視対象の`.agents/artifacts/`に保存する。ユーザー資料の`.orca/`は読まず、変更しない。

既存IRと設計契約の変更は通常の実装範囲に含めない。
実装と矛盾したために要求を緩めることはせず、停止条件に従う。

## Step order and prerequisites

S1、S2、S3、S4、S5、S6、S7、S8、S9、S10、S11の順に進む。
各ステップを複数の一関心コミットに分けてよい。
新しい振る舞いは先に失敗する試験を実行し、実装後に同じ試験を通す。
挙動不変の抽出・移動・削除は既存試験と型検査で確認し、移動しただけのコードに人工的な失敗試験を作らない。
既存試験の期待値を移動先の結果に合わせて変えない。

計画を承認・コミットしてから、そのローカルHEADを基点にブランチ`public-library-api`とworktree `.agents/worktrees/public-library-api/`を作る。
これは未pushの仕様・計画コミットを含む基点であり、remoteのmainからは分岐しない。
実際の基点SHAを開始記録に固定し、全差分レビューはそのSHAから候補HEADまでを対象にする。
作業treeの唯一の書込み担当をSolとし、レビューは読取り専用で行う。修正役へ渡す前に前の書込み担当を終了させる。
build出力とテスト用資源はこのworktree内に置き、主checkoutと共有しない。

着手時に[PROJECT.md](../../PROJECT.md)の環境条件と既存CIの準備を確認し、下記のworkspace検査を一度実行して基準を記録する。
TUN、pasta、nftables、Landlock、名前空間、bwrapの不足をskipで隠さない。
一般ユーザーの私有名前空間で実施し、ホストのOS設定やネットワーク規則を変更しない。
既存CIで得られる依存の準備は再利用できるが、特権操作が必要なら停止する。

実装前の`kotowari check`は要求25件とシナリオ51件の試験不足、文書規模の通知18件のみだった。
規模通知を減らすための再分割はこの計画に含めない。
このリポジトリは`changes`検査を設定していないため、新しい変更記録制度を導入しない。

## Verification map

下表のlibrary要求25件と未延期シナリオ51件をすべて検証する。
各ステップで実際にその振る舞いを検証するテストへ要求・シナリオのマークを付ける。
既存要求に既にマークがあっても、新APIへの追加契約が証明済みだとはみなさない。

| Step | Requirements | Examples・証拠 |
|---|---|---|
| S1 | REQ-library-102、REQ-library-103、REQ-library-401 | EX-library-103、104、105、106、402。policyだけの外部パッケージ、共通検証、型によるモード必須化 |
| S2 | REQ-150、REQ-454、REQ-154、REQ-399、REQ-467、REQ-305、REQ-306、REQ-307、REQ-308、REQ-309、REQ-315 | 既存マーク付き試験を維持。REQ-150・454は依存グラフと実コードの責務レビュー。新APIの分岐はS3以降でも検証 |
| S3 | REQ-library-101、REQ-library-104、REQ-library-105、REQ-library-403 | EX-library-101、102、107、108、109、405。起動を含む例はS4で完結 |
| S4 | REQ-library-106、REQ-library-201、REQ-library-202、REQ-library-203、REQ-library-204、REQ-library-301、REQ-library-303、REQ-library-407、REQ-library-409 | EX-library-110、111、201、202、203、204、205、206、207、301、302、303、306、307、410、411、415。まずhost・none、filteredはS6 |
| S5 | REQ-library-406、REQ-library-408、REQ-library-409、REQ-library-410、REQ-library-402、REQ-446、REQ-449、REQ-453、REQ-475 | EX-library-404、408、409、412、413、414、416、417。D8・D9・D12の実ファイル、最終制限、子孫回収も確認 |
| S6 | REQ-library-201、REQ-library-202、REQ-library-203、REQ-library-204、REQ-library-206、REQ-library-301、REQ-library-402、REQ-148 | EX-library-201から208、211、212、301、302のfiltered適用と入れ子。既存の外側ガード継承・重複拒否も回帰確認 |
| S7 | REQ-library-205、REQ-library-302、REQ-068、REQ-288 | EX-library-209、210、213、214、304、305。実際の通知と制御切断、有限保持、Pending取消し、最終結果の独立性 |
| S8 | REQ-library-303、REQ-library-304、REQ-library-401、REQ-library-404 | EX-library-306、307、308、309、401、406。workspace外のパッケージのビルド・実行 |
| S9 | REQ-library-402、REQ-library-410、REQ-284、REQ-285、REQ-288、REQ-291、REQ-401、REQ-352、REQ-148、REQ-068、REQ-150、REQ-454 | EX-library-403、418。従来CLIの全回帰試験、exec・終了コード・表示・端末・設定、責務レビュー |
| S10 | REQ-library-401、REQ-library-403、REQ-library-404、REQ-library-406、REQ-library-407、REQ-library-408、REQ-library-304 | 実際にビルドして動かした公式例、ガイドと現在のAPI・IRの照合 |
| S11 | 上記全件 | workspaceと独立例の検査、対象IDのマーク、別コンテキストの仕様・設計・差分レビュー |

表中のEX-libraryの数字だけを列挙した箇所は、先頭と同じ接頭辞を補って参照する。
新APIの試験はルートの統合試験から独立した利用側バイナリを起動し、そのバイナリの同期mainでdispatchする。
Rustのテストランナー自身を再実行して補助処理にする方式は使わない。
失敗コンパイルの試験はまず正しい利用側プログラムがコンパイルできることを確認し、対象の所有権・非公開性・必須引数を破った場合だけ拒否されることを見る。
PreparedRunとRunningのClone要求も、それぞれコンパイルで拒否されることを確認する。
存在しない依存や構文ミスによる失敗をAPIの保証の証拠にしない。

制御障害は所有している試験用プロセスの終了、通信端の切断、実際のFD・ポート競合で作る。
main前の初期化で子を作る専用の利用側fixtureも、通常のLinux ELF初期化として再現する。
同時実行の順序は通信による合図で固定し、短いsleepの偶然に依存しない。
非公開のテスト専用の同期手段を使う場合も、実際の状態遷移を通し、本番公開APIへ試験用操作を加えない。

## Left to the implementer

- 公開設計契約に名前がない内部型・ファイル・関数の命名と、同じ責務内の分割。
- 挙動不変の抽出順序と、最終的に取り除くcoreの一時的な転送層。重複した検証・計画実装は残さない。
- 試験の共有fixtureと内部同期の配置。既存の名前空間・ネットワーク・端末fixtureを先に探して再利用する。
- 公式非同期例のTokioの版はRust 1.88で検証できる版を選び、例のlockfileに固定する。製品ライブラリの依存とMSRVは変えない。
- 新しい入力型の補助メソッドは設計契約の全項目を表現するためのものに限る。許可範囲、既定値、停止の意味、起動・結果の分類を変える選択は委任しない。

## Stop conditions

- 承認済み要求同士の矛盾、未定の入力・エラー・寿命の意味、公開署名や制約の変更が必要になった場合は、根拠箇所と選択肢を添えてオーケストレータへ戻す。
- 記述子マウント、dev-bind後の同一性検査、役割付きコピー、最終Landlock下の起動確認、子孫回収が成立しない場合は実行したコマンドと観測を報告する。機能を削る、依存を自動公開する、ディスクへ退避することで通さない。
- 特権・不可逆・危険な対象への操作、別作業への事故の波及、変更した方法でも進展しない状態は停止する。レビューでsecurityに分類された指摘も利用者へ返す。
- 環境不足と製品不具合を区別し、必要な環境を用意できず試験が未実行なら完了と報告しない。フックの無効化、失敗試験の削除、要件マークだけの付替えはしない。
- 1回の委譲に収まらない場合は、完了コミットと各ステップの試験出力、未完了項目を`.agents/artifacts/`へ残して返す。オーケストレータは証拠のある段階からSolへ再委譲する。

## Test command

基本検査は[PROJECT.md](../../PROJECT.md#implementation-and-verification)の順で実行する。
着手前とS11では以下をすべて実行し、途中は変更した振る舞いの試験とその依存先の回帰試験に絞る。

```sh
cargo build --workspace --locked
cargo test --workspace --all-targets --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
```

新しい統合試験のコマンドは`cargo test --locked --test library_policy`と`cargo test --locked --test library_api`とする。
個別のREDとGREENでは同じコマンドへ対象のテスト名を付ける。
シグナル設定の検証ではprepare前後と起動・終了前後で、アプリケーションが利用可能なハンドラと呼び出し元のシグナルマスクの不変を確認する。
生の/proc/self/statusのビット集合の一致だけを保証全体の判定には使わない。
差分があれば使用するランタイム・libcの内部予約範囲と通常のスレッド生成による初期化の観測に照合し、ランタイム・libc自身の内部予約シグナル初期化だけであることを確かめる。
GNU・muslの予約範囲を同一と見なさず、特定のシグナル番号の一律除外、事前のスレッド起動による変化の隠蔽、libcハンドラの復元で通さない。
クレート内へ移した試験は`cargo test --locked -p <移動先クレート>`で確認する。
rootのall-targetsに加え`cargo test --workspace --doc --locked`を実行して公開例とコンパイル契約を確認する。

既存のpolicy・plan試験の多くはrootのintegration testsにあるため、新クレート単位のtestだけで移動の検証を終えない。
途中の回帰確認には次を使う。試験をクレート内へ移す場合は、同じ試験の移動先コマンドと理由を証拠に残す。

```sh
cargo test -p kakoi --locked --test policy --test layers --test variables --test mounts --test placement --test isolated_env --test plan
cargo test -p kakoi --locked --test cli --test guard --test launch --test nested --test listed_mounts --test listed_commands --test shared_files
cargo test -p kakoi --locked --test kakoi_net
```

`tests/kakoi_net/`は一つのtest targetにまとめられているため、個別実行は`cargo test -p kakoi --locked --test kakoi_net dns_name::`のようにmodule名で絞る。

S8以降は次を3例それぞれについて実行する。
ここで`<example>`は`library-policy`、`library-sync`、`library-async`のいずれかである。

```sh
cargo build --manifest-path examples/<example>/Cargo.toml --locked
cargo test --manifest-path examples/<example>/Cargo.toml --all-targets --locked
cargo fmt --manifest-path examples/<example>/Cargo.toml --all --check
cargo clippy --manifest-path examples/<example>/Cargo.toml --all-targets --locked -- -D warnings
cargo run --manifest-path examples/<example>/Cargo.toml --locked -- --self-test
```

各例は独立workspaceとし、rootのmembersには含めない。
runtimeだけへの直接依存の確認は同期例、policyだけへの依存の確認はpolicy例で行い、非同期例だけがTokioを追加する。
依存追加・移動時にはCargoの通常操作でlockfileを更新し、その後の検証は`--locked`で行う。
MSRV検査は既存CIのRust 1.88の方法を使い、workspaceと3例に適用する。
既存CIのGNU・musl両方のworkspace検証を維持し、自己再実行の例も両targetで確認する。
動的依存不足の試験はGNUの動的リンクした利用側で行い、静的リンクのmuslだけで代用しない。
既存のroot release buildとCLI成果物の場所を維持し、profileをinclude_strで同梱する相対参照も新クレートからビルドできることを確かめる。
統合試験からCargoを呼ぶ場合、生成先は当該worktreeのtarget配下で分離し、外側Cargoが保持するbuildロックを再取得しないようにする。
targetを`/tmp`へ置かない。

S11の最後に`kotowari check --format json`と対象IDの`kotowari query ID`を実行する。
この計画のID、またはブランチで変更したファイルに属するエラーとguide_staleがなく、review検証以外の対象要求・シナリオに実際のテストのマークがあることを確認する。
既存の対象外問題だけを理由に全リポジトリの`complete true`は求めない。

## Out of scope

crates.io公開、版上げ、リリース、ガード共有方式、別配布helper、新規PTY管理、新しい永続化・通信の公開形式は含めない。
push、mainへのmerge、ブランチ・worktreeの削除は最終成果の受入れ後に利用者が判断する。
自走の完了は、実装ブランチのコミット、検証証拠、収束したレビュー結果と残る観測事項を報告するところまでとする。

## Steps

### S1: 純粋なpolicyと外部からの入力契約

- Purpose: 下位の検証を一本化し、後続のクレートが同じ検証済みポリシーを使えるようにする。
- Specification: `docs/ir/library/library-input.md#REQ-library-102`, `docs/ir/library/library-input.md#REQ-library-103`, `docs/ir/library/library-integration.md#REQ-library-401`, `docs/ir/core/core-policy.md#REQ-399`, `docs/ir/core/core-listed-mounts.md#REQ-467`
- Prerequisites: 承認済み計画のコミット、専用worktree、既存検査と環境条件の基準記録。
- May change: `Cargo.toml`, `Cargo.lock`, `crates/kakoi-policy/`, `crates/kakoi-core/`の型・解析・合成・ガード照合と転送、関連import、`tests/library_policy.rs`, `tests/fixtures/library-api/`, `examples/library-policy/`, `.kotowari/config.yaml`の試験探索。
- Done when: policyだけの外部パッケージからRustとTOMLを検証でき、既存の全入力項目を表現できる。共通検証の同じ不正条件が両入口で拒否され、Rustのモード省略がコンパイルで拒否される。既存CLIの設定試験も通る。
- Shown by: test `cargo test --locked --test library_policy`、policy例のbuild・self-test、既存の設定・合成・ガード照合試験。公開入力項目の対応は設計契約と実装をレビューして確認する。
- Left to the implementer: 型名が未指定の入力部品の命名と、既存型を利用する内部表現。
- Stop and hand back if: 共通検証へ統一するために既存TOMLの受理範囲・省略値・合成順序を変更する必要がある。

### S2: plan・linux・netの責務分離

- Purpose: OSの観測と純粋な計画を分け、CLIと新APIが同じ判断処理を利用できるようにする。
- Specification: `docs/ir/network/network-library-boundary.md#REQ-150`, `docs/ir/core/core-command-guard-runtime.md#REQ-454`, `docs/ir/core/core-runtime.md#REQ-305`, `docs/ir/core/core-runtime.md#REQ-306`, `docs/ir/core/core-runtime.md#REQ-307`, `docs/ir/core/core-runtime.md#REQ-308`, `docs/ir/core/core-runtime.md#REQ-309`, `docs/ir/core/core-runtime.md#REQ-315`
- Prerequisites: S1。新APIの追加契約は後続で実装し、この抽出では既存CLIの経路を維持する。
- May change: `crates/kakoi-core/`, `crates/kakoi-plan/`, `crates/kakoi-linux/`, `crates/kakoi-net/`, `crates/kakoi-runtime/`の手順接続、rootと各crateのmanifest・lockfile、既存試験のimport、`.kotowari/config.yaml`の試験探索。
- Done when: 計画へ必要な観測値を渡せ、policy・planにOS操作が残らない。netが上位runtimeへ依存せず、DNS判断とOS操作が分かれる。既存の計画・マウント・listed・通信試験が同じ期待値で通る。
- Shown by: check `cargo build --workspace --locked`、移動元と移動先の既存試験、`cargo metadata --format-version 1 --no-deps`と実コードによるREQ-150・REQ-454の責務レビュー。
- Left to the implementer: 挙動を変えない抽出の細分化と、S9で除去するcoreの転送層。
- Stop and hand back if: 下位クレートからruntime・CLIへ依存しないと実装できない、またはCLIとAPIに同じ判断を複製する必要がある。

### S3: 同一バイナリのワーカーとprepare

- Purpose: 呼び出し元の環境・cwd・rlimitとREQ-library-201の対象となるシグナル設定を維持し、説明可能な計画と保持資源を一つの所有者へ結び付ける。
- Specification: `docs/ir/library/library-input.md#REQ-library-101`, `docs/ir/library/library-input.md#REQ-library-104`, `docs/ir/library/library-input.md#REQ-library-105`, `docs/ir/library/library-integration.md#REQ-library-403`, `docs/ir/core/core-policy.md#REQ-154`
- Prerequisites: S2。公開署名は設計契約の「要求と準備」、内部転送は「起動の成立と通信経路」を読む。
- May change: `crates/kakoi-runtime/`, `crates/kakoi-linux/`, `crates/kakoi-plan/`の観測値入力、manifest・lockfile、`tests/library_api.rs`, `tests/library_api/`, `tests/fixtures/library-api/`, `examples/library-sync/`。
- Done when: 同期mainでdispatchする独立した利用側からprepareと明示config入口を利用できる。非UTF-8入力、要求のcwd・PATH、FD所有、説明の秘密値抑制が成立する。prepare破棄と準備途中の通信失敗でもワーカーを回収し、呼び出し元の環境・cwd・rlimitを変えない。シグナル設定はREQ-library-201に従い、アプリケーションが利用可能なハンドラと呼び出し元のマスクを変えず、ランタイム・libc自身の内部予約シグナル初期化だけを例外とする。
- Shown by: test `cargo test --locked --test library_api`の準備・外部コンパイル試験。別profileのある環境、破棄、FD数と子の終了、非公開計画の書換え拒否を観測する。起動を含むEX-library-107・108・109はS4でも確認する。
- Left to the implementer: 内部IPCと回収担当のモジュール配置。設計契約にある上限と所有者の分離は変更しない。
- Stop and hand back if: 準備時に対象コマンドを動かす、設定を暗黙に読む、REQ-library-201で除外したランタイム・libc自身の内部予約シグナル初期化以外に呼び出し元のグローバル状態を変える必要がある。

### S4: 記述子マウントとhost・noneの寿命

- Purpose: 最小の実行経路で、計画の成立条件と起動・停止・回収の契約を製品試験にする。
- Specification: `docs/ir/library/library-input.md#REQ-library-106`, `docs/ir/library/library-lifecycle.md#REQ-library-201`, `docs/ir/library/library-lifecycle.md#REQ-library-202`, `docs/ir/library/library-lifecycle.md#REQ-library-203`, `docs/ir/library/library-lifecycle.md#REQ-library-204`, `docs/ir/library/library-observation.md#REQ-library-301`, `docs/ir/library/library-observation.md#REQ-library-303`, `docs/ir/library/library-integration.md#REQ-library-407`, `docs/ir/library/library-integration.md#REQ-library-409`
- Prerequisites: S3。内部設計の「マウント元の同一性」「停止と失敗時の回収」、D9・D12を読む。
- May change: `crates/kakoi-runtime/`, `crates/kakoi-linux/`, `crates/kakoi-plan/`の実行記述、`tests/library_api.rs`, `tests/library_api/`, `tests/fixtures/library-api/`, `examples/library-sync/`。
- Done when: host・noneで起動と同期wait、停止、Drop、所有者死亡、並行実行が成立する。bind-fd機能不足、リンク先置換、dev-bindの照合不一致では対象コマンドを動かさない。通常内容のライブ更新、spawnだけの共有ファイル準備、未読パイプ中の停止、主コマンドと回収の別結果を観測できる。
- Shown by: test `cargo test --locked --test library_api`。実bwrapでの機能確認と保持対象の置換、役割付きinit、所有者・ワーカー強制終了、子孫回収、exec失敗と非0終了を確認する。独立した利用側fixtureでprepare後に標準FDを別の対象へ差し替え、Inheritで起動したコマンドが準備時の対象を使うことを確認する。EOFだけの起動成否と未確認回収を成功扱いしない試験を含める。EX-library-202では「Test command」の方法でアプリケーションが利用可能なハンドラと呼び出し元のマスクの不変を確認し、観測した例外がランタイム・libc自身の内部予約シグナル初期化だけであることを確かめる。
- Left to the implementer: 実行所有者ごとの内部資源型と試験の同期手段。デバイス試験は私有名前空間で既存の無害なデバイスを使う。
- Stop and hand back if: 実ファイル・デバイスの同一性を最終配置で確認できない、所有者死亡で実行が残る、未確認の回収をConfirmedとして報告しないとAPIが成立しない。

### S5: コピーしたガードとlistedの接続

- Purpose: 補助役の実行許可と利用側アプリの許可を分け、ガードの配置から起動確認までを完成させる。
- Specification: `docs/ir/library/library-integration.md#REQ-library-406`, `docs/ir/library/library-integration.md#REQ-library-408`, `docs/ir/library/library-integration.md#REQ-library-409`, `docs/ir/library/library-integration.md#REQ-library-410`, `docs/ir/library/library-integration.md#REQ-library-402`, `docs/ir/core/core-command-guard-runtime.md#REQ-446`, `docs/ir/core/core-command-guard-runtime.md#REQ-449`, `docs/ir/core/core-command-guard-runtime.md#REQ-453`, `docs/ir/core/core-listed-commands.md#REQ-475`
- Prerequisites: S4。D8・D9・D12と内部設計の「役割の振り分けと起動の確認」を読む。
- May change: `crates/kakoi-runtime/`, `crates/kakoi-linux/`, `crates/kakoi-plan/`の配置計画、policyの既存照合との接続、`tests/library_api/`, `tests/fixtures/library-api/`, `examples/library-sync/`。
- Done when: 全配置コピーを最終listed制限下で実行でき、表消失・不正役割・inode不一致は通常アプリへ戻らない。元アプリを暗黙に実行許可せず、自身を対象とするガードも適用する。依存不足・実行拒否では対象コマンドが副作用を残さず、probeが作った子孫も解放前に回収される。
- Shown by: test `cargo test --locked --test library_api`。複数配置、実名末尾の削除済み接尾辞、動的依存が見える場合と欠ける場合、失われた表、probe子孫、ホストへのバイナリ非保存を実行して確認する。既存ガード・listed試験も通す。
- Left to the implementer: 役割情報の内部符号化と読取り実装。異常を通常アプリへ戻さない意味と配置方式は承認済み設計に従う。
- Stop and hand back if: 制限適用前にprobeする必要がある、コピーの実行にOS設定変更が必要、依存の暗黙公開や共有方式への変更が必要。

### S6: filteredと入れ子の監督

- Purpose: 既存通信の判断・終了手順を利用し、全モードの組み込み契約を完成させる。
- Specification: `docs/ir/library/library-lifecycle.md#REQ-library-201`, `docs/ir/library/library-lifecycle.md#REQ-library-202`, `docs/ir/library/library-lifecycle.md#REQ-library-203`, `docs/ir/library/library-lifecycle.md#REQ-library-204`, `docs/ir/library/library-lifecycle.md#REQ-library-206`, `docs/ir/library/library-observation.md#REQ-library-301`, `docs/ir/library/library-integration.md#REQ-library-402`, `docs/ir/network/process/process-filtered-supervision.md#REQ-148`
- Prerequisites: S5、PROJECT.mdのpasta・TUN・nftablesと私有名前空間の試験条件。
- May change: `crates/kakoi-net/`, `crates/kakoi-runtime/`, `crates/kakoi-linux/`の接続、`tests/library_api/`, `tests/kakoi_net/`の共有fixture、`tests/fixtures/library-api/`。
- Done when: filteredで通信遮断と回収を区別し、所有者死亡・通信障害・公開ポート競合でも他実行を止めない。入れ子の成功と作れない場合の拒否を観測し、外側制限・外側ガードの継承と重複拒否を維持する。
- Shown by: test `cargo test --locked --test library_api`のfiltered・入れ子試験と既存ネットワーク・nested試験。実通信を発生させ、終了後の到達不能とプロセス終了を別々に確かめる。
- Left to the implementer: netの診断を構造化結果へ渡す内部接続と、既存のネットワークfixtureの共用方法。
- Stop and hand back if: 新APIだけ通信の許可・復帰・停止規則を変更する必要がある、または入れ子で隔離を省略しなければ起動できない。

### S7: 有限イベントとキャンセル可能な待機

- Purpose: 観測者が遅い場合や待機を中断した場合も監督と最終結果の取得を成立させる。
- Specification: `docs/ir/library/library-lifecycle.md#REQ-library-205`, `docs/ir/library/library-observation.md#REQ-library-302`, `docs/ir/network/notification/network-notification.md#REQ-068`, `docs/ir/core/core-process.md#REQ-288`
- Prerequisites: S6。設計契約の「実行の所有と操作」「状態と結果」「起動の成立と通信経路」を読む。
- May change: `crates/kakoi-runtime/`, `crates/kakoi-net/`の通知接続、`tests/library_api/`, `tests/fixtures/library-api/`。
- Done when: 同期とFutureが同じ最終結果を返し、Pending取消しは受信位置を進めない。親側とワーカー側の欠落、独立した受信位置、満杯時の停止、制御切断時の未確認結果、複数waitが成立する。
- Shown by: test `cargo test --locked --test library_api`。実通知を受信限界以上発生させる試験と、製品の待機登録を通る競合試験を使う。結果確定の直前・途中・直後、Ready未取得で取消した場合、残ったStopHandle・Eventsが実行を延命しないことも確認する。
- Left to the implementer: 標準同期部品の内部配置と、公開契約に影響しない通知のまとめ方。
- Stop and hand back if: 非同期ランタイムの製品依存、無制限の保持、イベント受信に依存した停止が必要になる。

### S8: 独立した利用例と非同期I/O

- Purpose: 利用側が内部クレートを組み立てずに公開APIを使用できることを、実際の外部パッケージで示す。
- Specification: `docs/ir/library/library-observation.md#REQ-library-303`, `docs/ir/library/library-observation.md#REQ-library-304`, `docs/ir/library/library-integration.md#REQ-library-401`, `docs/ir/library/library-integration.md#REQ-library-404`
- Prerequisites: S7。S1・S3で追加した外部例を拡張し、同期mainからのdispatchを維持する。
- May change: `examples/library-policy/`, `examples/library-sync/`, `examples/library-async/`, `tests/library_api/`, `tests/fixtures/library-api/`, `crates/kakoi-runtime/`の公開I/O接続、`.github/workflows/`の例の検証。
- Done when: 3例がroot workspaceに属さずビルド・実行でき、同期例はruntimeだけ、policy例はpolicyだけに直接依存する。非同期例でパイプ所有権をAsyncFdへ移し、非ブロッキング読書きと終了を確認できる。利用者が用意したPTYのFD接続と継承も成立する。
- Shown by: test 「Test command」の3例のbuild・test・fmt・clippy・self-testと`cargo test --locked --test library_api`。Send・Syncの成功、PreparedRunとRunningそれぞれのClone拒否、移動済み計画の再利用拒否、入力モード、公開型だけでの構築を外部コンパイルで検証する。
- Left to the implementer: 各例のself-testの表示と、利用側だけのTokio設定。例を自動実行できる最小の補助引数は製品CLIへ追加しない。
- Stop and hand back if: runtime以外の内部クレートへの直接依存、async main内部でのdispatch、製品によるPTY作成が必要になる。

### S9: CLIを接続して旧coreを取り除く

- Purpose: 新しい責務分割を最終構成にし、既存CLIと組み込みAPIの共用を完成させる。
- Specification: `docs/ir/library/library-integration.md#REQ-library-402`, `docs/ir/library/library-integration.md#REQ-library-410`, `docs/ir/core/core-process.md#REQ-284`, `docs/ir/core/core-process.md#REQ-285`, `docs/ir/core/core-process.md#REQ-288`, `docs/ir/core/core-output.md#REQ-291`, `docs/ir/core/core-output.md#REQ-401`, `docs/ir/core/core-product.md#REQ-352`, `docs/ir/network/network-library-boundary.md#REQ-150`, `docs/ir/core/core-command-guard-runtime.md#REQ-454`
- Prerequisites: S8。一時的な転送層の呼出し先を確認してから削除する。
- May change: `src/`, `crates/`の共用接続とcore削除、rootと各crateのmanifest・lockfile、`tests/`のimport、`.github/workflows/`の移動に伴う参照、`.kotowari/config.yaml`の試験探索。
- Done when: CLIがruntime経由で共通処理を使い、coreへの依存と実装重複がない。host・noneのexec、filteredの監督と通知、入れ子既定exec、自己除外、設定・計画・終了コード・端末の既存契約が維持される。
- Shown by: check `cargo build --workspace --locked`、`cargo test --workspace --all-targets --locked`、`cargo metadata --format-version 1 --no-deps`。既存回帰試験の期待値変更がないことと、REQ-150・454の実コードを独立レビューする。
- Left to the implementer: CLI専用の接続APIの内部構成。正式な組み込みAPIに生の変更可能な実行計画を公開しない。
- Stop and hand back if: CLIにも新APIのbwrap条件・コピー方式・監督形態を強制しないと共用できない、または既存設定を移行しないと動かせない。

### S10: 実装と一致する利用案内

- Purpose: 利用条件と実際に動かせる例を揃え、利用者が公開APIから開始できるようにする。
- Specification: `docs/ir/library/library-integration.md#REQ-library-401`, `docs/ir/library/library-integration.md#REQ-library-403`, `docs/ir/library/library-integration.md#REQ-library-404`, `docs/ir/library/library-integration.md#REQ-library-406`, `docs/ir/library/library-integration.md#REQ-library-407`, `docs/ir/library/library-integration.md#REQ-library-408`, `docs/ir/library/library-observation.md#REQ-library-304`
- Prerequisites: S9と3例の実行成功。承認時の決定記録・設計文書を実装報告で上書きしない。
- May change: `README.md`, `PROJECT.md`, `docs/guide/`, `examples/library-policy/README.md`, `examples/library-sync/README.md`, `examples/library-async/README.md`。
- Done when: 現在のクレート構成と実行手順へ案内が揃い、main前初期化、コピー量、動的依存、機能検査、Dropとwait、非同期I/Oの条件を参照できる。実装済み部分の草案表示だけを更新し、設計履歴は残す。
- Shown by: artifact ガイドと3例のREADME、実行済みコマンドの出力、IRとguideマークの照合、`kotowari check --format json`のguide_staleなし。
- Left to the implementer: 説明の順序と例の表示。製品の条件や保証は増減しない。
- Stop and hand back if: 文書化に必要な利用条件が仕様にない、または例が承認済みの通常利用手順では動かない。

### S11: 全体検証と独立レビューへの引渡し

- Purpose: 実装したAPI、全既存CLI、外部利用、要求との対応を同じ候補HEADで確認する。
- Specification: `docs/ir/library/library-integration.md#REQ-library-401`, `docs/ir/library/library-integration.md#REQ-library-402`, `docs/ir/library/library-integration.md#REQ-library-404`, `docs/ir/network/network-library-boundary.md#REQ-150`, `docs/ir/core/core-command-guard-runtime.md#REQ-454`。検証対象の全要求はVerification mapに従う。
- Prerequisites: S10。未完了ステップ、未検証の必須環境、要求を弱める変更がない。
- May change: `.agents/artifacts/`の実装証拠のみ。失敗の修正は該当ステップの範囲と試験へ戻して別コミットで行う。
- Done when: 本計画の検証コマンドが成功し、対象のマーク不足・形式エラー・ガイド不整合がない。各ステップのコミット、RED/GREENまたは移動前後の証拠、公開契約の確認結果を保存し、レビューへ渡せる。
- Shown by: check 「Test command」のworkspace・doc・3例・MSRV検査、`kotowari check --format json`、対象IDの`kotowari query ID`、`git diff --check <base>..HEAD`、`git status --short`。オーケストレータは続けて全差分レビューと修正ループを実施する。
- Left to the implementer: 証拠ファイルの分類。必要な観測結果を省略しない。
- Stop and hand back if: 必須検査が未実行または失敗、結果の候補HEADが一致しない、要求の意味を変える修正が必要。
