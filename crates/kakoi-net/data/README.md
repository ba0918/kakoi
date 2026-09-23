# IANA特殊用途アドレスのスナップショット

DNS名だけでは許可しない範囲の判定に使う。起動時のダウンロードは行わない。
`iana-ipv4-special.txt` と `iana-ipv6-special.txt` は、次の登録表の
Address Block欄から脚注を除き、複数の範囲を1行ずつに分けたもの。

- [IPv4 Special-Purpose Address Space](https://www.iana.org/assignments/iana-ipv4-special-registry/)
- [IPv6 Special-Purpose Address Space](https://www.iana.org/assignments/iana-ipv6-special-registry/)

両表の更新日は2025-10-09。2026-09-16に取得したCSVのURL・SHA256・抽出件数を
`iana-special-provenance.json` に記録した。更新時には両方のデータと記録を更新し、
アドレス分類のテストを実行する。到達可能性の判定表としては使わない。

登録表の「Globally Reachable」が真の範囲も、製品仕様に従って明示許可を要求する。
ループバックの既定許可と、対象外のマルチキャスト・ブロードキャストは別に判定する。
IPv4-mapped IPv6はIPv4へ正規化してから分類する。
