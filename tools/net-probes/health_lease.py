"""Experimental finite kernel gate for the already measured pasta routes."""
import time

from guard import require_private_namespace
from recovery import nft, table


def renewer():
    require_private_namespace()
    nft([], table('recovery_policy', 'accept'))
    nft([], '''table inet health_lease {
 set live { type inet_service; flags timeout; }
 chain input { type filter hook input priority -20; policy accept;
  meta l4proto { tcp, udp } th dport 8080 th dport != @live drop
  meta l4proto { tcp, udp } th sport { 8080, 18081 } th sport != @live drop
 }
 chain output { type filter hook output priority -20; policy accept;
  meta l4proto { tcp, udp } th dport { 8080, 18081 } th dport != @live drop
  meta l4proto { tcp, udp } th sport { 8080, 18081 } th sport != @live drop
 }
}''')
    first = True
    while True:
        nft([], '''flush set inet health_lease live
add element inet health_lease live { 8080 timeout 2s, 18081 timeout 2s }
''')
        if first:
            print('ready', flush=True)
            first = False
        time.sleep(.1)


if __name__ == '__main__':
    renewer()
