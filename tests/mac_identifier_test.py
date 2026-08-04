# SPDX-License-Identifier: Apache-2.0

import time

import pytest

import nipart
from .testlib.cmdlib import exec_cmd
from .testlib.statelib import (
    load_yaml,
    show_only,
    show_saved_only,
    state_match,
)
from .testlib.veth import veth_interface

MAC_TEST_VETH = "veth-mac0"
MAC_TEST_VETH_PEER = "veth-mac1"
MAC_TEST_IP = "192.0.2.99"

ROUTE_MAC_NEXTHOP = "192.0.2.1"
ROUTE_LOGICAL_NAME = "my-gw-iface"


def _get_route_for_iface(iface_name):
    rc, out, _ = exec_cmd(
        ["ip", "route", "show", "dev", iface_name], check=False
    )
    return out


@pytest.fixture
def veth_env():
    with veth_interface(MAC_TEST_VETH, MAC_TEST_VETH_PEER):
        yield


def test_mac_identifier_resolve_with_veth(veth_env):
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
                  prefix-length: 24"""))

    iface_state = show_only(MAC_TEST_VETH)
    assert state_match(
        {
            "enabled": True,
            "dhcp": False,
            "address": [{"ip": MAC_TEST_IP, "prefix-length": 24}],
        },
        iface_state["ipv4"],
    )


def test_route_next_hop_interface_with_mac_identifier(veth_env):
    iface_state = show_only(MAC_TEST_VETH)
    mac_address = iface_state["mac-address"]

    nipart.apply(load_yaml(f"""---
        version: 1
        interfaces:
          - name: {ROUTE_LOGICAL_NAME}
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
        routes:
          config:
            - destination: 0.0.0.0/0
              next-hop-interface: {ROUTE_LOGICAL_NAME}
              next-hop-address: {ROUTE_MAC_NEXTHOP}
              table-id: 254
              metric: 199"""))

    time.sleep(1)

    route_output = _get_route_for_iface(MAC_TEST_VETH)
    assert (
        "default via" in route_output
    ), f"Route not found on {MAC_TEST_VETH}: {route_output}"
    assert (
        ROUTE_MAC_NEXTHOP in route_output
    ), f"Next hop {ROUTE_MAC_NEXTHOP} not found in route: {route_output}"

    saved_iface = show_saved_only(ROUTE_LOGICAL_NAME)
    assert (
        saved_iface is not None
    ), f"Saved config should be keyed by logical name {ROUTE_LOGICAL_NAME}"
    assert saved_iface["name"] == ROUTE_LOGICAL_NAME, (
        f"Saved config name should be {ROUTE_LOGICAL_NAME}, "
        f"not resolved kernel name"
    )

    saved_state = nipart.NipartClient().query_network_state(
        nipart.NipartQueryOption.saved()
    )
    saved_routes = saved_state.get("routes", {}).get("config", [])
    saved_route = next(
        (
            r
            for r in saved_routes
            if r.get("next-hop-interface") == ROUTE_LOGICAL_NAME
        ),
        None,
    )
    assert saved_route is not None, (
        f"Saved route should reference logical name {ROUTE_LOGICAL_NAME}, "
        f"not kernel name"
    )
    assert (
        saved_route["next-hop-interface"] == ROUTE_LOGICAL_NAME
    ), "Saved route next-hop-interface should be profile name"
