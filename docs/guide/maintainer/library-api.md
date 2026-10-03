# Rustから隔離を起動する

公開入口は`kakoi-runtime`と`kakoi-policy`である。
CLIを子プロセスとして呼ばずに、要求の準備、起動、停止、結果取得を行える。

## path依存と実例
<!-- @kotowari[REQ-library-401:690e007e, REQ-library-404:9294e69b] -->

利用側のCargo.tomlに、checkout内のクレートへのpath依存を追加する。
基本利用ではruntimeだけを直接依存にする。
ポリシーの検証だけならpolicyだけを使える。
crates.ioからの導入はこの手順に含めない。

```toml
[dependencies]
kakoi-runtime = { path = "../kakoi/crates/kakoi-runtime" }
```

独立したパッケージの実例は次の3つで、root workspaceには属さない。
各例のREADMEにビルドと実行の手順がある。

- [ポリシーの構築と検証](../../../examples/library-policy/README.md)
- [同期実行とパイプ](../../../examples/library-sync/README.md)
- [Tokioへのパイプ接続](../../../examples/library-async/README.md)

内部構成は[責務と実証条件](library.md)を参照する。
CLI専用の`cli`接続層は不安定な内部入口であり、組み込み先の実行手順には使わない。

## 同期mainで補助役を振り分ける
<!-- @kotowari[REQ-library-403:768ead7c, REQ-library-408:52a3923f] -->

同期mainの先頭で`dispatch_helper()`を呼ぶ。
`Dispatch::Application`だけがアプリの初期化へ進み、`Completed(code)`はそのcodeを返して終了する。
エラーでも通常のアプリ処理へ戻らない。
非同期アプリも、振り分け後にランタイムを構築する。
`#[tokio::main]`の内側で振り分ける例は使わない。

補助役は同じ実行ファイルを再実行する。
ELFの初期化関数など、mainより前に動く処理も再実行されるため、その副作用を利用側で考慮する。
自己実行ファイルを読み取れる必要がある。
ガードと隔離内initには役割付きのコピーを配置し、通常アプリとは区別する。
コピー量は実行ファイルのサイズと配置数に依存する。
設定表を失ったガードは終了126になり、通常アプリへ戻らない。

## 環境とポリシーを明示する
<!-- @kotowari[REQ-library-101:2bd00234, REQ-library-102:02c4a23a, REQ-library-103:9cf67121, REQ-library-104:e403952c] -->

`RunRequest::new`には検証済みPolicy、CommandSpec、HostContext、StdioSpecを渡す。
PolicyはTOMLからもRustの入力型からも同じ検証を通す。
Rustの構築ではマウント、ネットワーク、環境の各モードを明示する。
必要なポリシー型はruntimeから使える。

HostContextは要求の絶対cwdとOS文字列の環境を持つ。
`capture()`で呼び出し元から明示的に取得するか、`new()`へ値を渡す。
名前のコマンドはポリシー適用後の隔離用PATHで探索し、ガード配置も反映する。
ホスト側bwrapはポリシー適用前のHostContextのPATHで探す。
相対コマンドと相対workspaceは要求のcwdを基準にする。

メモリ入力の`prepare()`は既存profileを探さない。
ファイル設定を使う場合は`config::load(selection, context)`を明示的に呼ぶ。
共有ファイルの置き場も要求の環境から決まる。

## 準備と起動の条件
<!-- @kotowari[REQ-library-105:5dedcc5f, REQ-library-106:cd7b3998, REQ-library-406:68910231, REQ-library-407:9579c7b1, REQ-library-409:ff0025fe] -->

`prepare()`はワーカーと保持資源を作るが、対象コマンドはまだ実行しない。
PreparedRunの`description()`で説明を確認し、`spawn()`へ所有権を渡す。
計画は編集、複製、再利用できず、説明にも秘密値や実行用FD番号を含めない。
保持したマウント元と指定の解決結果が変わった場合は起動を拒否する。
通常のファイル内容とディレクトリの子孫はライブのままである。

組み込みAPIはbwrapの`--bind-fd`と`--ro-bind-fd`を検査する。
版名だけでは判断せず、不足時は起動エラーになる。
CLIの従来の対応条件は変わらない。
動的リンカと共有ライブラリをポリシーの外から自動公開しない。
補助役の依存を明示して見せるか静的リンクを選び、補助役を起動できない場合は対象コマンドの前にエラーになる。
既に依存が見える構成では追加指定は不要である。

非入れ子のspawnでは、要求のXDG_RUNTIME_DIRで既存規則に従った共有ファイルを準備する。
prepareだけでは作成せず、ガード用実行ファイルをその場所へ保存しない。
ガードのコピーを実行できなくても、ディスク退避やOS設定変更で自動的に回避しない。

## 実行の所有と結果
<!-- @kotowari[REQ-library-201:c7a3be6b, REQ-library-202:b2e8de92, REQ-library-203:e2ee962b, REQ-library-204:d1126ca1, REQ-library-301:42ea078d] -->

Runningが実行の唯一の公開所有者である。
StopHandle、Events、取り出したパイプを保持しても、Runningの寿命を延ばさない。
`request_stop()`は要求の記録を返し、回収完了までは保証しない。
`wait()`は回収後の確定結果を返し、複数回呼んでも同じArcの結果を返す。
RunningのDropは停止を開始するが、回収を待たない。
回収担当はDrop後もワーカーをwaitする。

RunOutcomeのmain、reason、network、processesを別々に読む。
主コマンドの終了を確認できても、通信遮断や全子孫の回収を確認できたとは限らない。
確認できない項目はUnknownまたはUnconfirmedになる。
filteredの停止では通信遮断を先に行う。
hostとnoneのnetworkはNotApplicableである。
呼び出し元のcwd、環境、rlimit、アプリ用シグナルハンドラと呼び出し元のマスクは変更しない。
ランタイムとlibc自身の内部予約シグナルの初期化だけはこのシグナル保証の例外である。

## イベントとキャンセル
<!-- @kotowari[REQ-library-205:16b3d10d, REQ-library-302:3bd3f285] -->

`events()`ごとの受信位置は独立し、作成時点以降を受信する。
`recv()`と`recv_async()`はEvent、Lagged、Closedを返す。
ネットワーク通知には状態と診断本文があり、本文を省略した場合はtruncatedで示す。
イベント保持は最大256件かつ1 MiB、個々のイベントは最大8 KiBである。
ワーカー側と親側の欠落を通し番号から識別する。
現在状態と最終結果は履歴と別に保持するため、受信が遅くても停止と結果取得を妨げない。

`wait_async()`と`recv_async()`は特定のランタイムを必須としないFutureである。
PendingのFutureを破棄しても実行は停止しない。
イベントを返すReadyになったときだけ受信位置を進め、Pendingからのキャンセルでは未取得イベントを消費しない。
保持上限で失った履歴は、キャンセルとは別にLaggedで知らせる。

## パイプと外部の端末
<!-- @kotowari[REQ-library-303:0087303f, REQ-library-304:519525c6] -->

stdin、stdout、stderrをそれぞれInherit、Pipe、Null、Fdで指定する。
Inheritはprepare時のFDを複製し、その後の差替えには追従しない。
FdはOwnedFdの所有権を渡す。
Pipeの利用側端はRunningから一度だけ取り出せる。
出力の収集は利用側が担当し、状態イベントとは混ざらない。

PipeReaderとPipeWriterは標準のReadまたはWrite、AsFd、OwnedFdへの変換を提供する。
非同期例では所有権をFileへ移し、O_NONBLOCKを設定してTokioのAsyncFdへ渡す。
パイプのEOFは書込み端を閉じて送る。
PTYは利用側が作り、そのFDを渡すか継承する。
kakoiは新規PTYを作成管理しない。

## 入れ子と自身へのガード
<!-- @kotowari[REQ-library-206:d2f14c25, REQ-library-402:359804b4, REQ-library-410:e26c0b3a] -->

組み込みAPIの入れ子も新しい隔離を作り、作れない場合は起動を拒否する。
外側の制限と既存のガードを維持する。
利用側アプリ自身をガード対象に指定した場合も規則を適用する。
CLIの既存の入れ子実行と、kakoi自身へのガード除外は維持する。
