# SPDX-License-Identifier: Apache-2.0

import time

import pytest

import nipart
from .testlib.cmdlib import exec_cmd
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml, show_only

MAC_TEST_VETH = "veth-mac-plug0"
MAC_TEST_VETH_PEER = "veth-mac-plug1"
TEST_MAC = "02:00:00:00:00:01"
MAC_TEST_VETH_REPLUG = "veth-replug0"
MAC_TEST_VETH_REPLUG_PEER = "veth-replug1"
MAC_TEST_VETH_REPLUG_NEW = "veth-replug2"
MAC_TEST_VETH_REPLUG_NEW_PEER = "veth-replug3"
MAC_TEST_PROFILE = "mac-replug0"
TEST_MAC_REPLUG = "02:00:00:00:00:7e"
TEST_MTU = 1280
MAC_TEST_IP = "192.0.2.99"
ROUTE_NEXTHOP = "192.0.2.1"
DEFAULT_TIMEOUT = 30


def _iface_has_mtu(iface_name, mtu):
    iface_state = show_only(iface_name)
    if iface_state is None:
        return False
    return iface_state.get("state") == "up" \
        and iface_state.get("mtu") == mtu


def _iface_has_route(iface_name):
    rc, out, _ = exec_cmd(
        ["ip", "route", "show", "dev", iface_name], check=False
    )
    return "default via" in out and ROUTE_NEXTHOP in out


@pytest.fixture
def mac_plug_env():
    exec_cmd(
        f"ip link add {MAC_TEST_VETH} address {TEST_MAC}"
        f" type veth peer name {MAC_TEST_VETH_PEER}".split()
    )
    exec_cmd(f"ip link set {MAC_TEST_VETH_PEER} up".split())
    yield
    exec_cmd(f"ip link del {MAC_TEST_VETH}".split(), check=False)
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {MAC_TEST_VETH}
          type: ethernet
          state: absent"""))


def test_mac_identifier_plugin_plugout(mac_plug_env):
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: wan0
          type: ethernet
          state: up
          identifier: mac-address
          mac-address: {TEST_MAC}
          mtu: {TEST_MTU}
          ipv4:
            enabled: true
            dhcp: false
            address:
            - ip: {MAC_TEST_IP}
              prefix-length: 24
        routes:
          config:
          - destination: 0.0.0.0/0
            next-hop-interface: wan0
            next-hop-address: {ROUTE_NEXTHOP}
            table-id: 254
            metric: 199"""))

    iface_state = show_only(MAC_TEST_VETH)
    assert iface_state["state"] == "up", \
        f"Expected {MAC_TEST_VETH} to be up, got {iface_state.get('state')}"
    assert iface_state["mtu"] == TEST_MTU, \
        f"Expected MTU {TEST_MTU}, got {iface_state.get('mtu')}"
    assert _iface_has_route(MAC_TEST_VETH), \
        f"Expected default route via {MAC_TEST_VETH} after apply"

    exec_cmd(f"ip link del {MAC_TEST_VETH}".split())
    time.sleep(1)

    assert not _iface_has_route(MAC_TEST_VETH), \
        f"Route should be gone after {MAC_TEST_VETH} removed"

    exec_cmd(
        f"ip link add {MAC_TEST_VETH} address {TEST_MAC}"
        f" type veth peer name {MAC_TEST_VETH_PEER}".split()
    )
    exec_cmd(f"ip link set {MAC_TEST_VETH_PEER} up".split())

    assert retry_till_true_or_timeout(
        DEFAULT_TIMEOUT, _iface_has_mtu, MAC_TEST_VETH, TEST_MTU
    ), f"{MAC_TEST_VETH} not up with MTU {TEST_MTU} after plugin"
    assert retry_till_true_or_timeout(
        DEFAULT_TIMEOUT, _iface_has_route, MAC_TEST_VETH
    ), f"default route via {MAC_TEST_VETH} not restored after plugin"


def test_mac_identifier_replug_with_new_kernel_name():
    # Same scenario as the dock replug: a MAC-identified NIC is removed and
    # comes back under a different kernel name.  The daemon must notice it
    # via the saved MAC watch and restore the profile without a restart.
    for iface in (
        MAC_TEST_VETH_REPLUG,
        MAC_TEST_VETH_REPLUG_PEER,
        MAC_TEST_VETH_REPLUG_NEW,
        MAC_TEST_VETH_REPLUG_NEW_PEER,
        MAC_TEST_PROFILE,
    ):
        exec_cmd(f"ip link del {iface}".split(), check=False)

    try:
        exec_cmd(
            f"ip link add {MAC_TEST_VETH_REPLUG} address {TEST_MAC_REPLUG}"
            f" type veth peer name {MAC_TEST_VETH_REPLUG_PEER}".split()
        )
        exec_cmd(f"ip link set {MAC_TEST_VETH_REPLUG_PEER} up".split())

        nipart.apply(load_yaml(f"""---
            interfaces:
            - name: {MAC_TEST_PROFILE}
              kernel-iface-name: {MAC_TEST_PROFILE}
              type: ethernet
              state: up
              identifier: mac-address
              mac-address: {TEST_MAC_REPLUG}
              mtu: {TEST_MTU}
              ipv4:
                enabled: true
                dhcp: false
                address:
                - ip: {MAC_TEST_IP}
                  prefix-length: 24
            routes:
              config:
              - destination: 0.0.0.0/0
                next-hop-interface: {MAC_TEST_PROFILE}
                next-hop-address: {ROUTE_NEXTHOP}
                table-id: 254
                metric: 199"""))

        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT, _iface_has_mtu, MAC_TEST_PROFILE, TEST_MTU
        ), f"{MAC_TEST_PROFILE} not up with MTU {TEST_MTU} after apply"
        assert _iface_has_route(MAC_TEST_PROFILE), (
            f"default route via {MAC_TEST_PROFILE} missing after apply"
        )

        # Remove the NIC, then bring it back with a different kernel name.
        exec_cmd(f"ip link del {MAC_TEST_PROFILE}".split())
        time.sleep(1)
        assert not _iface_has_route(MAC_TEST_PROFILE), (
            f"Route via {MAC_TEST_PROFILE} should be gone after removal"
        )

        exec_cmd(
            f"ip link add {MAC_TEST_VETH_REPLUG_NEW}"
            f" address {TEST_MAC_REPLUG}"
            f" type veth peer name {MAC_TEST_VETH_REPLUG_NEW_PEER}".split()
        )
        exec_cmd(f"ip link set {MAC_TEST_VETH_REPLUG_NEW_PEER} up".split())

        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT, _iface_has_mtu, MAC_TEST_PROFILE, TEST_MTU
        ), (
            f"{MAC_TEST_PROFILE} not restored with MTU {TEST_MTU} after "
            "replug under a new kernel name"
        )
        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT, _iface_has_route, MAC_TEST_PROFILE
        ), (
            f"default route via {MAC_TEST_PROFILE} not restored after "
            "replug under a new kernel name"
        )
    finally:
        for iface in (
            MAC_TEST_PROFILE,
            MAC_TEST_VETH_REPLUG,
            MAC_TEST_VETH_REPLUG_PEER,
            MAC_TEST_VETH_REPLUG_NEW,
            MAC_TEST_VETH_REPLUG_NEW_PEER,
        ):
            exec_cmd(f"ip link del {iface}".split(), check=False)
        nipart.apply(load_yaml(f"""---
            interfaces:
            - name: {MAC_TEST_PROFILE}
              type: ethernet
              state: absent"""))
