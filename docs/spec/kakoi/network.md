# ネットワークモード

Status: Draft（責務分割とネットワーク改訂の統合。承認待ち）

## 7. ネットワーク

network.modeはhost、none、filtered。hostはホストのネットワークをそのまま使う。noneはネットワーク名前空間を切り、ループバックだけが残る。

filteredは[通信ポリシー](network/policy.md)に従い、IP/CIDR/DNS由来のIP・TCP/UDP・ポートで通信を制限する。[DNS](network/dns-trust.md)、[ホスト接続](network/host.md)、[公開](network/publish-target.md)、[障害復帰](network/recovery.md)の細則を各文書で定義する。

host/noneの従来の挙動を維持する。noneで起動した隔離から外部アドレスへの接続は失敗し、hostではホストと同じに振る舞う。反例はnoneでホストのインターフェースが見えること。filteredの成功条件と反例は各要求から参照する。
