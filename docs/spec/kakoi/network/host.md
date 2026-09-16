# ホスト宛て接続

草案。実装前に[実証の完了条件](../proof-gate.md)を満たす必要がある。

隔離環境内の`localhost`は隔離環境自身を指す。ホストで動くサービスにつなぐ場合は、ホスト専用の宛先を許可し、アプリから専用名で接続する。

## ホストのlocalhostにつなぐ

次の例は、ホストの127.0.0.1で待ち受けるTCP・8080番だけを許可する。

```toml
[network]
mode = "filtered"

[[network.allow]]
destination = { host-loopback = "ipv4" }
protocol = "tcp"
ports = ["8080"]
```

アプリには接続先として`host-v4.kakoi.internal:8080`を指定する。`localhost:8080`と書くと、引き続き隔離環境内へ接続する。

ホストlocalhost専用の宛先は `destination` 内に `host-loopback` を置き、`ipv4` または `ipv6` を指定する。前者はホストの127.0.0.1、後者はホストの::1を指す。両方を許可する場合は2件書く。`dns`・`ip`・`cidr` とは同時指定しない。

隔離環境内で `host-v4.kakoi.internal` と `host-v6.kakoi.internal` の固定名を提供し、それぞれホスト127.0.0.1と::1へ指定ポートで中継する。通常の `localhost` は隔離環境内を指すままとする。名前を知っていても、許可したTCP/UDP・ポート以外には接続できない。

ホスト接続専用名を通常の `dns` 許可に完全一致で指定したら、`host-loopback` の使用を案内する設定エラーにする。通常DNSのワイルドカードからホストループバックの許可は追加しない。

## ホストのその他のIPにつなぐ

ホストの非ループバックIPにあるサービスへの接続は、通常の宛先と同じくIP/CIDR・TCP/UDP・ポートを明示許可して扱う。ホストという理由で自動許可しない。ホストlocalhostへの接続は既存の専用指定を維持する。

## 関連文書

[ネットワーク仕様の入口](README.md) · [要求・反例との対応表](../../../decision/brainstorm/2026-09-16-kakoi-net-spec-map.md)
