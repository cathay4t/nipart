# SPDX-License-Identifier: Apache-2.0

import time

import pytest

import nipart
from .testlib.cmdlib import exec_cmd
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml, show_only

MAC_TEST_VETH = "veth-mac-plug0"
MAC_TEST_VETH_PEER = "veth-mac-plug1"
TEST_MAC = "00:23:45:67:89:1a"
TEST_MTU = 1280
DEFAULT_TIMEOUT = 30


def _iface_has_mtu(iface_name, mtu):
    iface_state = show_only(iface_name)
    if iface_state is None:
        return False
    return iface_state.get("state") == "up" \
        and iface_state.get("mtu") == mtu


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
          mtu: {TEST_MTU}"""))

    iface_state = show_only(MAC_TEST_VETH)
    assert iface_state["state"] == "up", \
        f"Expected {MAC_TEST_VETH} to be up, got {iface_state.get('state')}"
    assert iface_state["mtu"] == TEST_MTU, \
        f"Expected MTU {TEST_MTU}, got {iface_state.get('mtu')}"

    exec_cmd(f"ip link del {MAC_TEST_VETH}".split())
    time.sleep(1)

    exec_cmd(
        f"ip link add {MAC_TEST_VETH} address {TEST_MAC}"
        f" type veth peer name {MAC_TEST_VETH_PEER}".split()
    )
    exec_cmd(f"ip link set {MAC_TEST_VETH_PEER} up".split())

    assert retry_till_true_or_timeout(
        DEFAULT_TIMEOUT, _iface_has_mtu, MAC_TEST_VETH, TEST_MTU
    ), f"{MAC_TEST_VETH} not up with MTU {TEST_MTU} after plugin"
