# kakoi-net 実証ランナー

pastaを採用できるか調べるための実験です。製品の実装ではありません。
[実証の完了条件](../../docs/guide/maintainer/library.md)のうち、環境・静的なループバック公開・直接中継の制限・有限許可の一部を確認します。

## 実行

ホスト側のWSL2ターミナルで、リポジトリのルートから実行します。sudoは使いません。
Pythonは起動シムを挟まないよう実体の`/usr/bin/python3`を指定します。Python 3、unshare、nsenter、ip、nftと、使用するpasta実行ファイルが必要です。ランナーはインストールを行いません。

既にこの作業でビルドした候補を使う場合：

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/passt/pasta
```

標準パッケージのpastaがPATHにある場合：

```bash
/usr/bin/python3 tools/net-probes/run.py
```

別の版とも比較できます。`--pasta /path/to/pasta`で実行ファイルを指定してください。
pastaは呼び出された名前で動作を選ぶため、リンク先のpasstではなくpastaのパスを渡します。
古い版が今回のオプションを受け付けない場合も失敗と記録します。その結果だけで同等機能がないとは判断しません。

## 観測すること

| 試験 | 成功条件 |
|---|---|
| tap | TAP作成が成功し、記述子を閉じると消える |
| lifetime | IPv4/IPv6 × TCP/UDPで、許可前の新規通信は拒否、許可中は成功、期限後も既存通信は継続、新規通信は拒否 |
| hooks-ipv4 / hooks-ipv6 | ホスト役→内側と内側→ホスト役の直接中継について、TCP/UDPともnftのaccept→drop→acceptに合わせて成功→拒否→成功となる |
| pasta-ipv4 / pasta-ipv6 | 内側のループバック限定TCP/UDP待受に、外側のループバックから到達する。外側の非ループバックアドレスでは公開されない |

それぞれ別のuser/network/PID namespaceで実行します。外側の試験ネットワークも合成したものなので、実ホストのインターフェースや経路にルールを追加しません。通常のループバックをホスト役とし、試験用dummyインターフェースとその内側のpastaを使います。

各試験は最大35秒。完了・timeout・Ctrl-C時には、その試験のプロセス群を終了し、launcherを回収します。PID namespaceの終了により内部の残存プロセスとネットワークも破棄されます。内部workerを直接実行すると拒否します。

## 第2段階：直接中継の制限

初回4試験が済んでいる場合は、次の2試験だけ実行できます。

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/passt/pasta \
  --only hooks-ipv4 --only hooks-ipv6
```

内側のnft input/outputで試験ポートを許可・拒否し、通信結果とルールのカウンターを残します。このルールは内側だけのループバック通信も巻き込むため、そのまま製品に使える制御方法ではありません。目的は直接中継にも制御をかけられる場所があるかの確認です。TAP経由の通信、内部通信との区別、アプリの権限剥奪は別途確認します。ルールの削除・再作成は測定準備であり、切替中の漏れを防ぐ設計の証明には使いません。

## 標準パッケージ版との比較

取得済みのUbuntu 24.04パッケージを、システムへインストールせず試せます。
この版にない`--host-lo-to-ns-lo`だけを省き、同じ通信条件を検査します。
既定動作が要求を満たすかは結果で判断します。自動的なオプション省略は行いません。

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/noble-package/extracted/usr/bin/pasta \
  --pasta-loopback-mode default \
  --only pasta-ipv4 --only pasta-ipv6 \
  --only hooks-ipv4 --only hooks-ipv6
```

選んだモードはreportに、実際のオプションは各試験のログに残します。
この実行は配布バイナリの互換性の比較であり、インストール時のAppArmor設定等を含めた導入完了の証明ではありません。

## 第3段階：内部通信とホスト宛て通信を分ける

Ubuntu配布版を使う次の試験は、pastaの直接中継を両方向とも無効にし、TAP経由の送信を確認します。

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/noble-package/extracted/usr/bin/pasta \
  --pasta-loopback-mode default \
  --only boundary-ipv4 --only boundary-ipv6
```

必要コマンドに`setpriv`が加わります。これはアプリ役からLinuxの管理権限を取り除くために使います。アプリ役をサービスと同じuser/network namespaceへ入れ、その後に権限を落とします。親user namespaceに残った所有者は子を管理できるため、能力ビットの除去だけでは十分ではありません。

| 宛先・操作 | 期待する結果 |
|---|---|
| 内部localhostのTCP/UDP・8081番 | 全段階で成功 |
| ホストlocalhost専用の宛先と、ホストの非ループバック宛先のTCP/UDP・8081番 | 拒否→許可→拒否に追従 |
| アプリ役によるnftルール削除・lo停止・新しいuser namespace経由のルール削除・raw packet socket作成 | 権限不足で拒否 |

専用宛先は試験内のgatewayアドレスで代用し、名前解決は行いません。宛先ごとの許可ルールと拒否ルールのカウンターも確認します。隔離内のループバックだけを一律許可する構成の部分実証です。ICMPv6は近隣探索を成立させるためこの試験では許可します。

製品の起動・復帰競合、公開側の制御、全ICMP方針、bwrapを含む脱出耐性全体は未検証です。新しいuser namespace作成自体が拒否された場合は、作成後の権限まで検証できたとは扱いません。いずれも実ホストのネットワークとは別のnamespace内で行います。

## 結果

起動時に`.agents/tmp/kakoi-net-proof/<日時>/`を表示します。結果は`report.json`、各試験の詳細は`<試験名>.stdout`と`.stderr`です。結果ディレクトリは報告用に残します。既存のディレクトリは上書きしません。

- `PASS`：その部分試験が通った。
- `FAIL`：試験または前提条件が失敗。stderrで原因を区別する。
- `BLOCKED`：pastaが見つからず未実行。
- `TIMEOUT`：35秒で終わらず強制終了。

**全項目PASSでも、実証全体の合格ではありません。** reportの`full_gate`は`NOT_RUN`のままです。一般のホスト宛て接続、動的な追加・削除・競合・再利用、許可判定の全経路、アプリの権限、監督の故障・復帰、TTLゼロ、UDPアイドル期限、端末と終了契約、将来の一斉送信対応は後続試験です。

部分再実行は、たとえば`--only lifetime`または`--only pasta-ipv6`を付けます。複数回指定できます。

## 再利用したもの

| 部分 | 選択と理由 |
|---|---|
| 通信寿命 | 既存のflow-lifetime実験を再利用。今回観測した4条件を再実行できる |
| pasta公開 | 既存のpasta-loopback実験をIPv4/IPv6へ拡張。静的公開の成立を先に調べる |
| 隔離と終了 | util-linuxのunshareとPID namespaceを使用。独自のnamespace管理を作らない |
| 実験制御・結果 | Python標準ライブラリ。追加のテスト基盤への依存を増やさない |

検証用候補の元ソースは既存記録のcommit `3a890a678fbeb930d41274c0258c1905f43cc068`。実行したバイナリのSHA-256とversion出力を毎回記録します。現在のビルドのversion文字列がunknownでも、ハッシュで使用物を区別します。標準パッケージでの導入容易性と採用する最低版の確定は未完了です。

## 第4組：公開の追加・解除と接続寿命

ソースビルド候補と、同じソースからビルドした`pesto`を使います。
`pesto`はpastaの公開設定を実行中に変更するツールです。上流では実験扱いです。

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/passt/pasta \
  --pesto .agents/tmp/kakoi-net-probes/passt/pesto \
  --only boundary-ipv4 --only boundary-ipv6 \
  --only dynamic-ipv4 --only dynamic-ipv6
```

この組では、新候補のTAP境界を両アドレス族で揃えて再測定します。
動的公開はTCP/UDPについて、未公開→追加→解除→再追加→解除を確認します。
解除後に新しい接続が通らないこと、既存TCP接続が通信を続けられること、
解放された番号に別のTCP/UDPソケットをbindできることを調べます。
TCPの再bindには通常の待受と同じく`SO_REUSEADDR`を使います。
公開先の内側サービスはlocalhostだけで待ち受けます。外側もlocalhost限定です。

追加で`mount`が必要です。制御ソケットは、試験専用mount namespaceの
`/tmp`へ載せたtmpfs（メモリ上のファイル領域）内に作ります。
ホストの`/tmp`は変更しません。これは一時ファイル配置の部分実証であり、
アプリから制御ソケットを隠すことはまだ検証しません。
自動待受検出、競合時の代替番号選択、UDPアイドル期限、故障・復帰も対象外です。
結果が全てPASSでも実証全体は`NOT_RUN`のままです。

再利用の判断：公開変更は上流のpesto、隔離と終了は既存runner、
接続の観測はPython標準socket/selectを使います。既存の一往復で閉じるfixtureでは
既存TCPの維持を測れないため、接続を保持する小さなecho fixtureを追加しています。

初回の動的試験では両アドレス族とも、解除後のUDP通常bindがポート使用中で失敗しました。
現在はこの失敗時にUDPソケット表とSO_REUSEADDR付きbindの成否を記録し、
再公開まで確認します。SO_REUSEADDR付きbindが成功しても、通常bindの失敗は
試験終了時のFAILとして残します。これは原因の切り分け用で、要求の緩和ではありません。

## 第5組：UDPサービス終了後の公開先変更

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/passt/pasta \
  --pesto .agents/tmp/kakoi-net-probes/passt/pesto \
  --only dynamic-udp-ipv4 --only dynamic-udp-ipv6
```

外側の公開番号18080を、内側localhostのUDPサービスA（8080）から
B（8082）へ割り当て直します。Aのソケットは実際にcloseし、完了通知を
受けてからBを公開します。自動待受検出の代わりに試験がpestoを操作します。

以前から通信する相手を二つ用意し、片方はA終了後・未公開中にも送信、
もう片方は再公開まで送信しません。未公開中の送信エラーで古い通信状態が
消えるケースを区別します。さらに新しい相手からもBへの通信を確認します。
全クライアントのソケットを保持し、送信元ポートを同一のまま測定します。

各相手の再公開後の3回の送信について、Bから最低1回正しい応答があることと、
Aから応答がないことを確認します。各回のtimeout/refusedも結果に残します。
これは限られた回数で切替を観測する条件であり、UDPの無損失保証ではありません。
A終了後・未公開中は応答がないことを要求します。
第4組のUDP通常bind失敗は別の未解決結果として維持します。

隔離・起動・終了は第4組の機構を再利用します。サービス識別とclose完了通知だけを
Python標準socket/selectと既存の標準入出力で追加しました。製品の制御経路、
自動検出、アプリからの隔離、全実証の合格を示す試験ではありません。

## 第6組：古いUDP通信状態の期限後の回復

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/passt/pasta \
  --pesto .agents/tmp/kakoi-net-probes/passt/pesto \
  --only dynamic-udp-expiry-ipv4 --only dynamic-udp-expiry-ipv6
```

第5組のサービス終了・再割当を再利用し、候補の無通信期限との関係を調べます。
pasta起動前に、試験専用network namespaceの`nf_conntrack_udp_timeout`と
`nf_conntrack_udp_timeout_stream`を両方10秒に設定します。候補の`udp_get_timeout_params`
が起動時に読む値です。実ホストの値や候補バイナリは変更しません。

直後の3試行を記録してから全クライアントの送信を12秒止め、同じ送信元から
再度Bへの通信を測ります。待機前後の公開番号に対応するUDPソケット表も記録します。
1試験は引き続き35秒で打ち切り、通常は両アドレス族で約40秒です。

この試験のPASSは「期限後に各相手が3試行以内にBの正しい応答を受けた」という
部分結果です。直後の失敗は`immediate_failures`に残し、第5組の失敗を解消扱いにしません。
期限待ちを製品の解決策として採用する試験ではなく、原因を切り分けるためのものです。

## 第7組：UDPの後始末を追加した実験版との比較

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/passt-udp-cleanup-prototype/pasta \
  --pesto .agents/tmp/kakoi-net-probes/passt-udp-cleanup-prototype/pesto \
  --only dynamic-ipv4 --only dynamic-ipv6 \
  --only dynamic-udp-ipv4 --only dynamic-udp-ipv6
```

これまでと同じ試験で、バイナリだけを別ビルドへ替えます。
実験版は元候補のコピーに、明示的に削除された公開ルールに該当するUDPフローを
既存の`udp_flow_close`で片付ける処理を追加しています。
TCPと、引き続き公開ルールに一致するUDPフローは対象外です。
差分は`.agents/tmp/kakoi-net-probes/udp-cleanup-prototype.patch`にあります。

これは上流未採用の実験版です。製品での採用・配布・独自fork維持は決めていません。
対象は明示的な削除→追加の経路に限定し、ルールの宛先を直接変更する経路や
自動走査での待受消失は扱いません。今回の試験で他のUDP公開の継続・負荷時の競合まで
検証したことにはしません。元候補のバイナリと、これまでの失敗結果は維持します。

## 第8組：通信中の繰り返し変更と対象外通信の維持

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/passt-udp-cleanup-prototype/pasta \
  --pesto .agents/tmp/kakoi-net-probes/passt-udp-cleanup-prototype/pesto \
  --only dynamic-udp-churn-ipv4 --only dynamic-udp-churn-ipv6
```

第7組の実験版を変更せず、UDP公開の削除→追加を8回繰り返します。
内側のA（8080）とB（8082）は起動したまま、外側18080の公開先を交互に変えます。
同時に、変更対象UDPと対象外のC（外側18084、内側8084）へ別スレッドから送信します。
各ソケットの送信元ポートは保持します。Cと同じ番号のTCP接続も維持し、
各削除・追加後に同じ接続で応答を確認します。

各回、削除後は対象UDPから応答がなく、追加後は3送信以内に意図したA/Bから
応答があり、逆のサービスから応答がないことを確認します。
対象外Cは各ラウンド中に最低1回の正しい応答を要求し、別サービスの応答を拒否します。
バックグラウンド送信のtimeout/refused件数も記録しますが、UDP無損失は要求しません。
切替中の対象UDPのA/B応答や不達は診断用の集計で、切替完了後の判定と分けます。

この試験は並行通信下の有限回の観測です。全ての競合順序、任意負荷への耐性、
他のUDPフロー内部状態が一度も再生成されないこと、製品の全実証合格は保証しません。
既存のnamespace・起動・終了とpesto制御を再利用し、並行送信とA/B/C識別だけを
標準socket/select/threadingで追加しています。上流ソースへの追加修正はありません。

## 第9組：制御ソケットをアプリから隠す

```bash
/usr/bin/python3 tools/net-probes/run.py --only control-isolation
```

TUNやpastaを使わず、実際のUNIX待受ソケットとbwrapで配置の機構を測ります。
試験専用tmpfsに制御ソケットを作り、制御側は前後とも接続できることを確認します。
アプリ側はuser/mount/PID namespaceを分け、ネットワークは制御側と共有します。
アプリの`/tmp`を別tmpfsで覆い、`/proc`も作り直し、capabilityを除去します。

アプリから制御ソケットへ直接接続できないこと、見える各PIDの`/proc/<pid>/root`
経由でも接続できないこと、制御socketのfdを継承していないことを確認します。
さらに子user/mount namespaceから`/tmp`をunmountして接続する試みを行います。
子namespaceの作成自体が権限不足なら、その先の操作は未検証と明記します。

これは指定した経路の部分試験です。pasta/pestoとの統合、抽象UNIXソケット、
任意のファイルシステム別名、製品のseccomp構成、全脱出経路の検証ではありません。
必要コマンドにbwrap・mount・umountが加わり、既定試験には含めません。

## 第10組：pastaの制御ソケットとbwrapの統合

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/passt-udp-cleanup-prototype/pasta \
  --pesto .agents/tmp/kakoi-net-probes/passt-udp-cleanup-prototype/pesto \
  --only dynamic-control-ipv4 --only dynamic-control-ipv6
```

実際のpastaが作る設定変更用UNIXソケットを対象に、第9組のbwrap構成を適用します。
pastaが作った内側namespaceでbwrapアプリを起動し、別tmpfsで制御パスを覆います。
アプリからの直接/proc root経由アクセス、制御fdの継承を調べ、制御側は前後に
pestoで公開ルールの追加・削除を行います。削除後の状態も照合します。
この組は制御操作の統合試験であり、公開先サービスの通信は第7・8組で別に測ります。
全脱出経路や製品のseccompとの組合せは未検証です。

## 第11組：許可更新プロセスの停止と独立した遮断

```bash
/usr/bin/python3 tools/net-probes/run.py --only lease-stop --only lease-kill
```

TUNは不要です。IPv4/IPv6のループバック上で、実際の別プロセスがnftの2秒期限付き
許可を更新します。更新を2回行って初回の期限を越えても新規通信が通ることを確認後、
更新完了の通知を受けた状態でSIGSTOPまたはSIGKILLを送ります。
更新を実行する外部nftコマンドが処理中のケースは、この試験では対象外です。

2.3秒後に新規TCP/UDPが通らず、確立済みTCP/UDPは継続できることを確認します。
続いて別の制御役が既存通信より優先する遮断ルールを設定し、新旧両方が通信できない
ことを確認します。初期の許可なし状態も検査します。各試験は約12秒が目安です。

これは故障時の機構を測る部分試験です。製品の自動監視・停止期限・アプリ終了・
pastaとの組合せ・起動復帰の競合・端末契約を実装したものではありません。
既存のnft有限許可とsocket fixtureを再利用し、実プロセスの故障注入だけを追加しています。

## 第12組：使用中の公開番号と代替・再利用

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/passt-udp-cleanup-prototype/pasta \
  --pesto .agents/tmp/kakoi-net-probes/passt-udp-cleanup-prototype/pesto \
  --only dynamic-conflict-ipv4 --only dynamic-conflict-ipv6
```

試験の外側namespace内で、別サービスがlocalhostのTCP/UDP 18080を確保します。
既存公開18084を動かしたまま、pestoで競合する18080の公開を要求します。
元のホストサービスが応答を続け、18084の新規TCP/UDPと既存TCPが維持されることを測ります。
続けて競合ルールを削除し、代替18082で公開、元のサービスを終了した後には18080へ
再度公開します。試験は選んだ3番号だけを使用し、番号の自動探索は行いません。

pestoの終了コードと反映後の設定は観測値として記録します。設定に載ったことだけを
公開成功とは扱わず、通信と元サービスの応答を確認します。競合時の既存公開不達が
観測された場合は、その後回復しても最後にFAILとします。
元ホストTCPの再利用条件を揃えるため、その待受にもSO_REUSEADDRを指定します。
全て実ホストから分離したnamespace内で行い、実ホストのサービスには接続しません。

この候補のpestoはbind結果を終了コードで返さないため、競合要求の送信が成功し、
読み戻した設定に対象番号が載ったことを試験の前提とします。送信失敗や未反映は
競合経路を試せたと扱わずFAILとします。その上で実際の通信を評価します。

待受の確認方法も同じ組で測ります。pastaの `/proc/<pid>/fd` のsocket所有情報と、
外側network namespaceのTCP/UDP表を照合します。競合番号はpasta所有でないこと、
既存・代替・再利用した番号はpasta所有であること、削除後は待受が消えることを確認します。
TCPの確立済み接続とUDPの接続済みsocketは待受から除きます。
所有者の観測自体はアプリへデータを送らず、既存の通信試験を別の裏付けとして残します。

この方式はpastaのプロセス情報を制御側から読める配置が前提です。参照拒否はFAILとし、
追加権限やpastaの保護解除で迂回しません。これは指定時点の観測であり、番号割当の
競合を完全に防ぐ機構や製品の公開通知までを実証するものではありません。

## 第13組：pasta経路の遮断と遮断中の再構築

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/passt-udp-cleanup-prototype/pasta \
  --only recovery-ipv4 --only recovery-ipv6
```

公開ポート、内側からホストへの直接socket転送、TAP経由のホストループバック接続、
TAP経由の非ループバック接続について、新規・接続済みのTCP/UDPを測ります。
外部サービス役も分離した試験namespaceに置き、実ホストやインターネットには接続しません。

別プロセスの制御役がルールを設置した後、その制御役をSIGKILLで終了します。
独立した処理でinput/outputへ遮断ルールを設置し、通信が不達になることを確認します。
通常のルールを再作成しても遮断が続き、明示的に遮断を解除した後だけ新規通信が
復帰することを測ります。内側だけで完結する別ポートの通信は全段階で継続させます。

既存のnamespace探索、権限を落とすclient配置、echo fixture、プロセス回収を再利用します。
新設部分は各経路の接続を保持する試験役と故障・再構築の進行だけです。
待受の公開は静的設定で、pestoは使いません。制御役の終了を待ってから遮断するため、
自動監視や検知までの期限、書込み途中の故障、起動時の制限、DNS期限切れの非復活は
対象外です。判定はアプリとのデータ往復であり、代理側でのTCP handshake拒否や、
遮断前に送ったデータの破棄を保証するものではありません。全実証ゲートはNOT_RUNです。

## 第14組：起動時の遮断と生存通知による自動遮断

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/passt-udp-cleanup-prototype/pasta \
  --only watchdog-stop-ipv4 --only watchdog-stop-ipv6 \
  --only watchdog-kill-ipv4 --only watchdog-kill-ipv6
```

第13組の経路・通信観測・再構築手順を再利用します。内側の準備処理がサービスの
起動前に遮断ルールを置き、制御役を起動する前と、ルール準備後の明示解除前に
通信が不達であることを測ります。内側専用8082は継続します。

制御役は通常ルールを設置後、pipeへ100ミリ秒ごとに生存通知を送ります。
別の監視プロセスは最初の通知を受けてから準備完了を返します。通信成功の確認後、
制御役へSIGSTOPまたはSIGKILLを送り、通知が1秒途絶えるかpipeが閉じると監視役が
独立した遮断ルールを設置します。故障注入から遮断完了まで5秒以内であることと、
新規・既存通信の不達を確認します。1秒/5秒は試験の設定で、製品の既定値ではありません。

監視には標準ライブラリのpipe/select/単調時計と既存nft呼出しを使います。
監視役は遮断完了後に終了する試験用処理です。その後の再構築・明示解除は第13組と
同じ手動進行で、自動復帰や継続監視の完成を意味しません。
監視役自身の故障、nft書込み途中の故障、起動中の全タイミング、全ポート・全プロトコル、
期限切れDNS許可の非復活、製品の準備完了プロトコルは対象外です。
実ホストに変更は加えず、各試験は既存runnerの35秒の上限内で実行します。

## 第15組：許可期限を延ばさない再構築

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --only lease-recovery-ipv4 --only lease-recovery-ipv6
```

TUN不要です。検査済みDNS応答から得た許可の代わりに、2つのIPへ2秒/8秒の期限を
与えます。制御役の終了後に独立guardで遮断し、短い許可が切れてから別プロセスで
許可集合を再構築します。元の単調時計上の失効時刻を引き継ぎ、失効済みのIPを除きます。
再構築後もguardで遮断され、解除後には有効なIPだけ新規TCP/UDPが通ること、
元の長い期限を過ぎるとそのIPも新規通信が通らなくなることを測ります。

nftの相対期限が適用処理中に延びるのを避けるため、試験では残存期間から500msを
差し引き、適用完了まで500msを超えたら準備失敗にします。残り500ms以下の許可は
復元しません。この保守的な試験値は製品のTTL仕様・既定値ではなく、早期失効を伴います。
最終的な精度と短いTTLの扱いは別途確定が必要です。

期限入力は固定した試験データで、DNS解析や応答の信頼判定は実行しません。
第13組の通信fixture・独立guardと標準の単調時計・JSONを再利用します。
遮断を挟んだ既存TCP/UDPの再開、pastaとの統合、監視から自動復帰までの一連の動作は
この組の対象外です。製品コード・仕様・IRは変更しません。

## 第16組：監督役の終了とbwrap配下の回収

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --only process-parent-death --only process-main-exit
```

TUN不要です。bwrapの内側に、さらに別のsessionへ移った子サービスを置きます。
監督役のSIGKILL、または主コマンドの終了により、内側PID namespaceのプロセスが
全て消え、既存TCPと待受も使えなくなることを確認します。主コマンドの終了値23を
bwrap越しに保持することも測ります。外側の試験用PID 1が引き取った終了済みプロセスは
waitで回収します。実ホストのプロセスへのsignalは送信しません。

既存bwrapの `--die-with-parent` とPID namespace、echo fixture、procの観測を再利用します。
これは強制終了と通常のbwrap終了の機構試験です。終了猶予やCtrl+C、SIGTERMの終了値、
安全故障125の優先はまだ測りません。既定bwrapの主コマンド終了で子も即時終了するため、
子へ猶予を与える製品構成には別の監督処理が必要です。

## 第17組：カーネルに残る許可期限を保った復帰

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --only lease-preserve-ipv4 --only lease-preserve-ipv6
```

第15組と同じ故障・遮断・通信観測を使い、復帰時はnftの許可集合を残してchainだけを
再構築します。集合の識別番号が変わらないこと、失効済みの許可は消えていること、
有効だった許可も元の期限後には新規通信を通さないことを確認します。
復帰時にtimeoutを付け直さないため、第15組の復元時500ms控除は発生しません。
初期投入は同じ保守的なfixtureを使います。DNS応答受信から最初の投入までの
期限精度、カーネル状態を信頼できない故障からの復帰は別問題として残ります。

## 第18組：監視・制御役の再起動・期限切れ許可を含む復帰

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/passt-udp-cleanup-prototype/pasta \
  --only resume-ipv4 --only resume-ipv6
```

第14組の監視構成に、第17組のカーネル許可保持を組み合わせます。外向き通信の
試験用IP許可を2秒で失効させます。制御役の強制終了を監視役が検知して遮断した後、
試験の監督処理が別の制御役・監視役を起動します。実際の生存通知を受け、通常ルールが
再構築されても遮断が維持されることを確認してから再開します。

復帰後、ホスト接続・公開・内側通信は成功し、期限切れIPへの新規通信だけは不達を
要求します。さらに交代した制御役も強制終了して、交代した監視役による再遮断を測ります。
試験中に利用者の操作を挟まず、1回の交代と2回の故障を進行します。
DNS応答そのものは扱わず、2秒の許可は固定した検査済み応答の代わりです。
任意回数の再試行、製品の準備確認全体、監視役自身の故障、権限配置の全組合せは対象外です。

## 第19組：端末からのCtrl+C

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --only process-ctrl-c --only process-ctrl-c-default
```

TUN不要です。標準の疑似端末（PTY）へCtrl+Cのバイトを送り、カーネルが生成するSIGINTを
実際の監督処理・bwrap・アプリへ届けます。アプリが割り込みを処理して続行し、後で23を
返す場合と、通常のSIGINT終了で130を返す場合を測ります。
同じ端末グループのbwrapまでSIGINTで終了しないよう、ラッパーには無視設定を継承させ、
アプリで通常動作または独自handlerへ戻します。汎用ランチャーでもアプリ起動前の復元が
必要です。この試験は全端末安全策や任意アプリでの動作を保証するものではありません。

## 第20組：終了猶予・遅れて生まれる子・終了値の優先

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --only process-interrupt-grace --only process-repeat-term --only process-safety-exit
```

bwrapの `--as-pid-1` を使い、試験用の監督処理を内側PID 1として残します。
主コマンド終了後、別sessionの子はSIGTERMを無視し、猶予中にさらに子を作ります。
2秒の共通猶予で全てを回収します。次の3ケースを測ります。

- 主コマンドの23確定後にPTYからCtrl+Cを送ると猶予を短縮し、23を維持する。
- 実行中にSIGTERMを受け、主コマンドが後処理で0を返しても143を返す。再度のSIGTERMで
  最初の期限を変えず、新しい子にも同じ期限を適用する。
- 23確定後、capabilityを持たない内側で実際のnft投入がEPERMになる故障を注入し、
  猶予を打ち切って125を優先する。通常の権限配置や全通信障害を模したものではない。

OSのPID namespace・signal・wait・PTYと既存runnerを再利用します。試験用の監督処理は
製品実装ではありません。成功する通信遮断との統合、他の全シグナル順序、任意の端末設定は
別途検証が必要です。broadcast signalは自身がPID 1の分離namespace内でだけ送信します。

## 第21組：隔離したホストDNS proxyの中継と設定変更

`systemd-resolved` 255.4で、127.0.0.54のproxyへのTCP/UDP照会を測る。
未知のTYPE65280のRDATAと、Aに付けたRRSIGのRDATAが中継後も一致することを確認する。
RRSIGは形式を満たす合成データであり、暗号学的な署名検証ではない。
さらに、特定ドメインだけ別サーバーへ送るsplit DNS、そのドメイン設定のsnapshot変化、
上流変更後に返るIPの切替を確認する。

試験はprivate user/net/PID/mount namespaceで動く。`/run`、`/etc`、`/tmp`をprivate tmpfsで
覆い、専用D-Bus・resolved・2台のDNS fixtureだけを起動する。実ホストDNSは変更しない。
rootless試験ではsystemd-resolveをnamespace内uid 0へ割り当てるため、実サービスの
uid分離を確認するものではない。fixtureはIPv4だけで、DoT、DNSSEC検証、キャッシュ有効時の
挙動、変更時の進行中照会の扱い、kakoi自身の変更検出・世代切替は対象外。

追加の試験依存はsystemd-resolved、dbus-daemon、busctl、Python 3.10以上とdnspython 2.8.0。
Pythonライブラリは製品の依存ではない。例えばuvがある環境では作業用venvへ導入できる。

```bash
uv venv --python /usr/bin/python3 .agents/tmp/kakoi-net-probes/dns-proof-venv
uv pip install --python .agents/tmp/kakoi-net-probes/dns-proof-venv/bin/python \
  --index-url https://pypi.org/simple --require-hashes -r tools/net-probes/dns-requirements.txt
/usr/bin/python3 tools/net-probes/run.py --only dns-proxy \
  --dns-python .agents/tmp/kakoi-net-probes/dns-proof-venv/bin/python
```

`--only dns-proxy`は明示した場合のみ実行され、既存の既定8ケースには加わらない。
この部分試験のPASSだけで、ホストDNSの正式な対応構成や全実証gateの完了とはしない。

## 第22組：DNS許可の登録遅延と有効化

```bash
/usr/bin/python3 tools/net-probes/run.py --only lease-stage-ipv4 --only lease-stage-ipv6
```

初回のIP許可を、まず通信判定から参照されない集合へ登録する。
登録時間が控除した予算内だったと確認してから、その集合を参照するルールを有効にする。
これにより、相対timeoutの計算後に書込み役が遅れても、遅い登録だけで通信を許可しない。

合成した3秒の許可、500msの控除と予算、700msの書込み前遅延を用いる。
遅い登録後も新規TCP/UDPは拒否され、別IPの許可は維持することを測る。
通常の登録・有効化では新規通信ができ、書込み役が終了しても元期限後は新規を拒否し、
既存TCP/UDPは継続する。登録後の有効化自体を元期限後まで遅らせても、期限切れ要素を
再投入せず集合を参照するだけなので、新規通信が復活しないことも確認する。

これは初回許可の部分実証。500msを製品の期限精度として採用したものではなく、
実DNS、TTLゼロの具体値、多数の並行更新・同一IPの更新競合、全故障注入は含まない。

## 第23組：監視が遮断できない場合にも期限で通信を止める

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --only lease-heartbeat-stop --only lease-heartbeat-kill
```

各ケースでIPv4/IPv6のTCP/UDPを確認する。第10組の許可更新役を使い、2秒の有限集合を
通信系の生存期限として、確立済み通信を許可する規則より先に検査する。
更新役をSIGSTOPまたはSIGKILLした後、新しい遮断コマンドを実行する前に、
カーネルの期限切れだけで既存・新規の通信が止まることを測る。
通常のDNS許可失効で既存通信を維持する試験とは別の規則である。

これにより、生存期限を更新する補助処理も停止した場合に、監視役の遮断操作だけへ
依存しない機構を確認する。2秒は試験値で、製品値ではない。健康状態を確認してから
更新する最終的な監督構成、pasta全経路との統合、更新中の故障は未実証。

## 第24組：生存期限による遮断をpastaの各経路で確認する

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/passt-udp-cleanup-prototype/pasta \
  --only health-stop-ipv4 --only health-stop-ipv6 \
  --only health-kill-ipv4 --only health-kill-ipv6
```

第23組の有限な生存期限を、TAP・直接転送・公開の入出力hookへ置く。
起動guardの下で更新役と有限集合を準備し、guard解除後の既存・新規TCP/UDPを確認する。
更新役を停止・強制終了した後は追加の遮断コマンドを使わず、期限切れだけで各経路が
不達となり、隔離環境内の通信だけは続く必要がある。pastaが終了したため不達になった
場合や、期限集合が残っている場合は失敗とする。

値は試験用の2秒期限・100ms更新・故障後2.3秒待機。最終的な健康状態の判定、
更新処理中の故障、復帰はこの組の対象外。全体の実証gateは別判定のまま。

第24組のローカル補助試験（TUN不要）:

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --only lease-health-push-ipv4 --only lease-health-push-ipv6
```

接続済みサーバーからの一方的な送信も、健康期限後には届かないことを確認する。
要求・応答の往復だけでは、要求を落とした後もサーバー発のデータが通る不足を
検出できない。第24組の健康ルールは送信元・宛先の両側を検査する。
この補助試験はloopbackであり、pasta経由の一方的送信まで確認済みとはしない。

## 第25組：pasta各経路の一方的送信を遮断する

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/passt-udp-cleanup-prototype/pasta \
  --only health-push-ipv4 --only health-push-ipv6 \
  --only recovery-push-ipv4 --only recovery-push-ipv6
```

接続済みのサーバーへ、要求に応答する形ではなく一方的な送信を指示する。
遮断前は全経路でTCP/UDPのデータを受信でき、遮断後は内側通信だけが受信できる必要がある。
各サーバーから送信試行の完了通知も受け、試験用サーバーの未送信を不達と取り違えない。
TCPの一部のbyteだけ届いた場合も成功扱いしない。

`health-push-*`は更新役の強制終了後、有限集合の失効だけで遮断する。
`recovery-push-*`は独立guardを投入する。独立guardにも送信元ポートの照合を加えたため、
宛先方向だけを落とす従来の不足を同じ試験で検査できる。
既存の要求・応答、新規接続の不達と内側通信の継続も確認する。

固定した試験ポートと通信経路の部分実証であり、任意の内部通信を識別する製品用規則、
最終的な監督処理、復帰や終了猶予との統合は別途必要。

## 第26組：通信遮断を確認してから終了猶予を始める

```bash
/usr/bin/python3 tools/net-probes/run.py --only process-network-order
```

TUN不要。第20組のbwrap内PID 1による終了処理と、private net namespaceのnft guardを
組み合わせる。主コマンドの終了後、PID 1は遮断完了の通知を待ち、既存・新規TCP/UDPが
不達と確認できてから残った子へ終了要求を出して共通2秒の猶予を始める。
通知が4秒以内に来なければ、猶予へ進まず強制終了して125を返す。

猶予中も通信は不達で、後発の子を含めて全員を回収し、主コマンドの終了23を保つ必要がある。
遮断完了・通知受信・猶予開始の単調時計も比較する。試験のTCP/UDP端点はbwrapと同じ
private net namespaceのloopbackにあり、実pasta経路と最終監督構成の統合ではない。
4秒の通知待ちと2秒の猶予は試験値であり、製品の遮断待ち上限を確定するものではない。

## 初版実装の開始確認：固定公開と未修正pasta

動的公開の削除・再割当を初版から分離したため、通常の起動時設定だけを使う。

```bash
/usr/bin/python3 tools/net-probes/run.py \
  --pasta .agents/tmp/kakoi-net-probes/passt-debian-backports/root/usr/bin/pasta \
  --initial-runtime
```

初版の実通信を確認する10ケース（boundary、固定公開、固定公開競合、生存期限による
一方的送信の遮断、独立guardによる同遮断、それぞれIPv4/IPv6）を実行する。
従来の既定バッチは変更しない。`--only`と`--initial-runtime`は同時指定できない。
単独の固定公開試験には `--only fixed-ipv4-ipv4`、`--only fixed-ipv6-ipv6`、
`--only fixed-conflict-ipv4`、`--only fixed-conflict-ipv6` を使える。

上記の作業用パスにはDebian公式backportsの
`passt_0.0~git20260728.f8df3f1-1~bpo13+1_amd64.deb`を展開した。
[配布ページ](https://packages.debian.org/trixie-backports/amd64/passt/download)掲載のSHA256
`c8ac47eb51979289f8bbb1761d9954cf1ee8dd1ac42ea5d2ce5136330099f43a`と照合済み。
システムへインストールしていない。別環境ではこのパスが存在するとは限らないため、
公式パッケージから導入・展開した未修正pastaのパスを指定する。
UbuntuにDebianのaptリポジトリを追加する導入方法は推奨しない。
このパッケージでの機構観測と、対応OSの標準導入環境での製品検証は区別する。

固定公開の試験は、内側サービス起動前のTCP/UDP待受所有、サービス停止中の公開番号維持、
同じUDP送信元を使った再起動、環境終了後の普通のbindと次環境での再利用を測る。
競合試験は先に競合なしで起動できることを確認し、TUN不在や別の起動失敗を
競合検出の成功と取り違えない。サービス起動を許可する最終製品の監督処理は別工程。

交差系統の `fixed-ipv4-ipv6` / `fixed-ipv6-ipv4` はpastaの拒否を再現する診断用として残す。
初版バッチには含めない。初版は同一系統に限定し、両系統へ公開する場合は2件を明示する。
この10ケースのPASSだけでDNS・依存選択・最終権限構成を含む工程1全体や製品完成を宣言しない。
