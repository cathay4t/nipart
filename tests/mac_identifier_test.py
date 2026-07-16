# SPDX-License-Identifier: Apache-2.0

import time

import nipart
from .testlib.cmdlib import exec_cmd
from .testlib.statelib import load_yaml, show_only, state_match
from .testlib.veth import veth_interface

MAC_TEST_VETH = "veth-mac0"
MAC_TEST_VETH_PEER = "veth-mac1"
MAC_TEST_IP = "192.0.2.99"

ROUTE_MAC_NEXTHOP = "198.51.100.254"
ROUTE_LOGICAL_NAME = "my-gw-iface"


def _get_route_for_iface(iface_name):
    rc, out, _ = exec_cmd(
        ["ip", "route", "show", "dev", iface_name], check=False
    )
    return out


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


def test_route_next_hop_interface_with_mac_identifier():
    with veth_interface(MAC_TEST_VETH, MAC_TEST_VETH_PEER):
        iface_state = show_only(MAC_TEST_VETH)
        mac_address = iface_state["mac-address"]

        nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {ROUTE_LOGICAL_NAME}
                type: ethernet
                identifier: mac-address
                mac-address: {mac_address}
                state: up
                ipv4:
                  enabled: true
                  dhcp: false
            routes:
              config:
                - destination: 0.0.0.0/0
                  next-hop-interface: {ROUTE_LOGICAL_NAME}
                  next-hop-address: {ROUTE_MAC_NEXTHOP}
                  table-id: 254
            """))

        time.sleep(1)

        route_output = _get_route_for_iface(MAC_TEST_VETH)
        assert "default via" in route_output, (
            f"Route not found on {MAC_TEST_VETH}: {route_output}"
        )
        assert ROUTE_MAC_NEXTHOP in route_output, (
            f"Next hop {ROUTE_MAC_NEXTHOP} not found in route: {route_output}"
        )

        nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {ROUTE_LOGICAL_NAME}
                type: ethernet
                identifier: mac-address
                mac-address: {mac_address}
                state: absent
            routes:
              config:
                - destination: 0.0.0.0/0
                  next-hop-interface: {ROUTE_LOGICAL_NAME}
                  next-hop-address: {ROUTE_MAC_NEXTHOP}
                  state: absent
                  table-id: 254
            """))
