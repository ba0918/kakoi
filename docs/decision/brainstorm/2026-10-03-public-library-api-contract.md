# 公開Rust APIの設計契約

[合意記録](./2026-10-03-public-library-api.md)のD2で委任された型、操作、内部通信を具体化する承認前の設計案。
以下のRustは署名の一覧であり、実装済みコードではない。
内部クレートの型を使わずに要求の構築、説明の確認、起動、停止、結果取得を完結させる。

## ポリシーとファイル入力

```rust
impl Policy {
    pub fn from_toml(source: &str) -> Result<Self, PolicyError>;
    pub fn validate(input: PolicyInput) -> Result<Self, PolicyError>;
}
impl PolicyInput {
    pub fn new(mounts: MountPolicy, network: NetworkPolicy,
               environment: EnvironmentPolicy) -> Self;
}
```

Policyは検証済みの値で、フィールドの変更と未検証のDeserializeを公開しない。
PolicyInputは編集可能な入力で、mounts、network、environmentにはそれぞれモードを必須とするコンストラクターを設ける。
commandsは既存の省略時の意味であるhostから始め、listedとguardを指定できる。
PolicyInputとTOMLは同じ正規化済み入力を作り、同じ検証関数を通る。
TOMLだけが既存のモード省略規則を適用する。

既存のPolicyFileが持つ全項目を入力型へ対応付ける。
mountsの各directive、scan、hide-mounts、system、networkのallow、publish、limits、dns-upstream、allow-nested-filtered、processのshutdown-grace-seconds、envのpass、set、unset、path-prepend、secrets、instead-of、commandsのmode、allow、guardを欠落させない。
型名の変更で既存の合成順序や優先順位を変更しない。
Rustで渡すファイルパスにはPathBufを使い、既存の変数付きパスは別の式の型で表す。

設定ファイルの入口はruntime::configに置く。
load(selection, context)は既存のLayerSelection相当の選択と明示的なHostContextを受け、読み込んだPolicyと出典を返す。
ファイル探索、既定profileへのfallback、ファイル読込みを行うのはこの入口だけである。
メモリ入力のprepareは設定ファイルを探さない。
共有ファイルの準備はA25の既存例外を使い、prepareではなくspawn時に行う。要求のXDG_RUNTIME_DIRから場所を決め、ガード用のバイナリをそこへ書かない。
要求に渡された環境からHOMEと設定ディレクトリのパスを導出し、既存のパス変数と保護対象の計算に使うことは、設定の読込みと区別する。

## 要求と準備

```rust
pub fn dispatch_helper() -> Result<Dispatch, DispatchError>;
pub enum Dispatch { Application, Completed(std::process::ExitCode) }

impl HostContext {
    pub fn new(cwd: PathBuf, env: BTreeMap<OsString, OsString>)
        -> Result<Self, InputError>;
    pub fn capture() -> Result<Self, std::io::Error>;
}
pub enum Io { Inherit, Pipe, Null, Fd(OwnedFd) }
pub struct StdioSpec { pub stdin: Io, pub stdout: Io, pub stderr: Io }
impl CommandSpec {
    pub fn new(program: OsString) -> Self;
    pub fn arg(self, argument: OsString) -> Self;
}
impl RunRequest {
    pub fn new(policy: Policy, command: CommandSpec,
               context: HostContext, stdio: StdioSpec) -> Self;
    pub fn workspace(self, path: PathBuf) -> Self;
}
pub fn prepare(request: RunRequest) -> Result<PreparedRun, PrepareError>;
impl PreparedRun {
    pub fn description(&self) -> &PlanDescription;
    pub fn spawn(self) -> Result<Running, StartError>;
}
```

利用側は同期mainの先頭でdispatch_helperを呼ぶ。
Applicationだけが通常のアプリ初期化へ進む。CompletedとErrはmainから終了し、通常処理へ戻さない。
async mainの属性マクロの内側で呼ぶ例は提供せず、同期mainから非同期ランタイムを構築する例を提供する。

cwdは絶対パスを要求し、環境はこの要求の値だけを使う。
programが名前なら要求のPATH、相対パスなら要求のcwdを基準に既存規則で解決する。
workspace省略時は要求のcwdを使い、指定時の相対パスもcwd基準にする。
Homeやconfig_dirが必要な規則は、要求の環境から既存規則で解決できなければ入力エラーにする。
NULを含むOS文字列と、不正な環境変数名はOS呼出し前に拒否する。

PreparedRunとRunningはSendだがCloneを提供しない。
prepareはワーカーを起動して資源を保持し、その過程で呼び出し元のcwd、環境、rlimitを変更しない。
シグナル設定は[A28](./2026-10-03-public-library-api.md#A28)を反映した[REQ-library-201](../../ir/library/library-lifecycle.md#REQ-library-201)に従い、アプリケーションが利用可能なハンドラと呼び出し元のマスクを変更しない。
スレッドランタイム・libc自身による内部予約シグナルの初期化だけを例外とし、特定のシグナル番号を全環境で除外しない。
prepareが成功しても対象コマンドはまだ動いていない。
StdioSpecのFdは所有権を移す。Inheritは準備時に呼び出し元の対応するFDを複製し、後で別のFDへ差し替わっても追従しない。
Pipeは起動時に作り、利用側の端をRunningに所有させる。

PlanDescriptionはポリシーの有効モード、解決されたコマンドとcwd、マウントの公開対象と権限、スキップした指定、ガード配置数、必要な外部機能、診断を読み取れる型とする。
秘密値は説明とDebugに含めない。
内部FD番号、ワーカーPID、bwrapの変更可能な引数列を実行用インターフェースとして公開しない。
descriptionの型からPreparedRunを復元する関数、永続化形式、Cloneによる再起動は提供しない。

## 実行の所有と操作

```rust
impl Running {
    pub fn status(&self) -> RunStatus;
    pub fn outcome(&self) -> Option<Arc<RunOutcome>>;
    pub fn request_stop(&self) -> Result<StopReceipt, ControlError>;
    pub fn stop_handle(&self) -> StopHandle;
    pub fn events(&self) -> Events;
    pub fn wait(&self) -> Arc<RunOutcome>;
    pub fn wait_async(&self) -> impl Future<Output = Arc<RunOutcome>> + Send + '_;
    pub fn take_stdin(&mut self) -> Option<PipeWriter>;
    pub fn take_stdout(&mut self) -> Option<PipeReader>;
    pub fn take_stderr(&mut self) -> Option<PipeReader>;
}
impl StopHandle {
    pub fn request_stop(&self) -> Result<StopReceipt, ControlError>;
}
impl Events {
    pub fn recv(&mut self) -> EventRead;
    pub fn recv_async(&mut self) -> impl Future<Output = EventRead> + Send + '_;
}
pub enum EventRead { Event(RunEvent), Lagged { missed: u64 }, Closed }
pub enum StopReceipt { Queued, AlreadyRequested, AlreadyFinished }
```

RunningはSyncも提供し、同期waitと別スレッドのrequest_stopを両立させる。
StopHandleはClone可能だが実行を所有しない。
PipeReaderとPipeWriterはそれぞれRead、WriteとAsFd、Into<OwnedFd>を提供する。
取り出したパイプの寿命も実行の所有権とは無関係である。
ライブラリはコマンド出力を自動的に収集しない。

Queuedは呼び出し元側の停止要求の記録を意味し、監督側の処理開始や回収完了の保証ではない。
一度記録された停止要求は繰り返し送れる状態に保ち、イベント通知の混雑で取り消さない。
通信障害で監督へ届けられない場合は生存確認接続を切断し、死亡追従の停止経路を作動させる。
待機の結果には通信障害と確認できなかった後処理を残す。

waitは複数回呼べ、確定した同じ結果を返す。
wait中の通知欠落、Futureの破棄、EventsやStopHandleの破棄は実行を終了させない。
イベント待機がPendingのまま破棄された場合は受信位置を進めない。イベントを返すReadyの時点でだけ受信位置を進め、保持上限による欠落はLaggedとして報告する。
RunningのDropは生存確認接続を閉じ、停止を開始する。回収待ちは行わない。
回収担当はハンドル破棄後もワーカーをwaitする。
呼び出し側が自身のwaitpid(-1)でライブラリの子を先に回収した場合は、確認できない終了状態を成功と見なさず、外部による回収として記録する。

## 状態と結果

状態はStarting、Running、Stopping、Finishedを区別する。
Startingはspawnが内部で所有し、spawnが成功して返すRunningはRunningまたは既にFinishedの状態である。
短いコマンドが戻り値の受取り前に終了することをエラーにしない。
StoppingからRunningへ戻らない。

RunOutcomeは以下を別々のフィールドとして保持する。

| フィールド | 値 |
|---|---|
| main | Exited(code)、Signaled(signal, core_dumped)、NotStarted、Unknown |
| reason | Completed、StopRequested、OwnerLost、InfrastructureFailure |
| network | ConfirmedBlocked、Unconfirmed、NotApplicable |
| processes | ConfirmedReaped、Unconfirmed |
| diagnostics | 起動・通信・回収に関する構造化された診断 |

networkのConfirmedBlockedはfilteredでのみ使用する。
noneとhostはNotApplicableとし、noneのネットワーク分離そのものを否定する意味にはしない。
ConfirmedReapedは隔離内の子孫の終了と、所有した監督・補助プロセスの回収を確認した場合にだけ使う。
主コマンドのstatusは生の終了コードまたはシグナルとして保持し、128+signalへの変換はCLI側だけで行う。
複数の終了原因を観測した場合はInfrastructureFailure、OwnerLost、StopRequested、Completedの順でreasonを選び、既に確認したmainの結果は失わない。

PrepareErrorとStartErrorはphase、kind、diagnostics、cleanupを持つ。
kindはInvalidInput、UnsupportedEnvironment、PlanChanged、ResourceConflict、HelperFailure、Io、Protocolを区別する。
cleanupはNotNeeded、Confirmed、Unconfirmedを区別し、部分起動した補助処理の回収状況を含める。
StartErrorにはmainの観測状況も含め、開始要求の直後の通信断をNotStartedと断定しない。
ResultのErrを返す前も、回収担当へ子の所有権を渡したままとし、単にChildを捨てない。
OSエラー番号と原因段階を機械的に読めるようにし、文字列の文面を判定条件にしない。

## 起動の成立と通信経路

ワーカーへの要求と応答には、既存依存のserdeとserde_jsonを内部の長さ付きメッセージとして使う。
OS文字列はUnixのバイト列として符号化し、UTF-8への損失変換をしない。
同じ利用側実行ファイルの再実行に限定し、プロトコルの版が一致しなければ拒否する。永続化と異なる版の接続は提供しない。
FDは既存のUnix socketとlibcのSCM_RIGHTSで移し、メッセージの宣言数と受信数が異なる場合は閉じて拒否する。
受信FDにCLOEXECを設定し、必要な子に必要なFDだけを渡す。

要求・制御、生存確認、イベントの経路を分ける。
ワーカーが読む制御メッセージは有限のサイズを宣言し、上限を超える要求は対象コマンドを動かす前に入力エラーにする。
上限は1メッセージ16 MiB、FDは1メッセージ16個とし、大きな資源集合は番号付きで分割する。
イベントは既存の通知上限に合わせて最大256件かつ1 MiBを保持する。各イベントは最大8 KiBとし、大きな診断本文は省略したことを明示する。
イベント番号は送信前に割り当て、ワーカー側と親側のどちらで捨てた場合も受信側が欠落数を認識できる。
Eventsは作成時点以降を受信し、複数のEventsは独立した読取り位置を持つ。
最終結果と現在状態はこのリングから独立して保持し、すべてのEventsがなくても更新する。
最終結果の通知はイベントの満杯を理由に捨てない。制御接続を失った場合は親側がUnconfirmedを含む結果を確定する。

spawnの成功は隔離と補助処理の準備が完了し、対象コマンドの起動処理が同期的なexec失敗を報告しなかったことを表す。対象コマンドが実際に何かを実行した保証とはしない。
execが報告したエラーはclose-on-execのエラーパイプで識別し、報告されたexec失敗と通常の非0終了を区別する。
エラーパイプのEOFだけでexec成立を確認したとは扱わない。fork後exec前に子が殺された場合とexec後のシグナル終了は、この仕組みだけでは区別できない。
その場合は起動用の子のシグナル終了を診断に残し、主コマンドの実行を確認できなければmainをUnknownとする。StartErrorがコマンド未実行を保証するのは、解放前の失敗が確認された場合だけである。
対象コマンドの解放前に、隔離のinit、listedの制限下での必要なガードの起動確認、filteredの通信制御が成立したことを確認する。
補助処理の起動確認はmainの振り分け処理へ到達したことを応答で確認する。ELFの依存名を静的に列挙しただけで成功とはしない。
起動確認用に渡したFDとモードは対象コマンドとその子孫に継承させない。
ガード規則が利用側アプリ自身を対象としている場合も、その規則を適用する。役割情報を付けていない元のアプリと、ガード用・init用のコピーは同じinodeとして扱わない。

## 再利用の判断

| 処理 | 採用するものと理由 |
|---|---|
| 型とポリシー検証 | 既存のPolicyFile、merge、検証を分離して再利用。別系統のTOML実装を作らない |
| FDの所有とパイプ | 標準ライブラリのOwnedFdと既存libc利用。非同期ストリームを新設しない |
| 待機と通知 | Mutex、Condvar、Future、Waker。監督処理は既に独立しているので非同期ランタイムを組み込まない |
| 内部メッセージ | 導入済みserdeとserde_json。公開された永続化形式ではなく同一バイナリ内の通信だけに使用 |
| FDの移動と生存確認 | Unix socket、SCM_RIGHTS、EOF。既存の制御接続の実装を基にする |
| イベント欠落の検出 | 通し番号と有限のリング。必要な状態機械だけをruntimeに置く |

## 実装へ渡す検証項目

外部パッケージのコンパイルで、runtimeだけへの直接依存、非UTF-8のOS文字列、SendとSyncの契約、計画の再利用不可を確認する。
実行試験では起動前失敗とexec後の非0終了、主コマンドと子孫の寿命、所有者死亡、待機キャンセル、遅い受信者、未読パイプ、並行起動の独立性を確認する。
待機登録と終了通知の競合は、登録の直前・途中・直後に結果が確定する条件を含める。
起動確認で依存不足を検出し対象コマンドが走らないことは、副作用を残すコマンドを用いて確認する。
この文書の署名と状態遷移は実装と外部コンパイル試験で検証する対象であり、現時点でコンパイル済みとは扱わない。
