# SPDX-License-Identifier: Apache-2.0

import nipart
from .testlib.statelib import load_yaml, show_only, state_match
from .testlib.veth import veth_interface

MAC_TEST_VETH = "veth-mac0"
MAC_TEST_VETH_PEER = "veth-mac1"
MAC_TEST_IP = "192.0.2.99"


def test_mac_identifier_resolve_with_veth():
    with veth_interface(MAC_TEST_VETH, MAC_TEST_VETH_PEER):
        iface_state = show_only(MAC_TEST_VETH)
        mac_address = iface_state["mac-address"]

        nipart.apply(load_yaml(f"""---
            interfaces:
              - name: my-veth
                type: ethernet
                identifier: mac-address
                mac-address: {mac_address}
                state: up
                ipv4:
                  enabled: true
                  dhcp: false
                  address:
                    - ip: {MAC_TEST_IP}
                      prefix-length: 24
            """))

        iface_state = show_only(MAC_TEST_VETH)
        assert state_match(
            {
                "enabled": True,
                "dhcp": False,
                "address": [{"ip": MAC_TEST_IP, "prefix-length": 24}],
            },
            iface_state["ipv4"],
        )
