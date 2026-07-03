# SPDX-License-Identifier: Apache-2.0

# TODO(Gris Ge): We should add test to ensure live changes will not bring
# interface down using `ip link monitor`.

import pytest

import nipart

from .testlib.statelib import load_yaml
from .testlib.statelib import show_only

TEST_BASE_NIC = "dummy1"
TEST_VXLAN_NIC = "vxlan100"


@pytest.fixture
def vxlan_over_dummy():
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_VXLAN_NIC}
                type: vxlan
                state: up
                vxlan:
                  id: 100
                  base-iface: {TEST_BASE_NIC}
                  learning: false
                  destination-port: 1235
              - name: {TEST_BASE_NIC}
                type: dummy
                state: up
            """))
    yield
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_VXLAN_NIC}
                type: vxlan
                state: absent
              - name: {TEST_BASE_NIC}
                type: dummy
                state: absent
            """))


def test_create_and_remove_vxlan(vxlan_over_dummy):
    vxlan_iface = show_only(TEST_VXLAN_NIC)
    assert vxlan_iface["vxlan"]["id"] == 100
    assert vxlan_iface["vxlan"]["base-iface"] == TEST_BASE_NIC
    assert vxlan_iface["vxlan"]["learning"] is False
    assert vxlan_iface["vxlan"]["destination-port"] == 1235


@pytest.fixture
def dummy2():
    nipart.apply(load_yaml("""---
            interfaces:
              - name: dummy2
                type: dummy
                state: up
            """))
    yield
    nipart.apply(load_yaml("""---
            interfaces:
              - name: dummy2
                type: dummy
                state: absent
            """))


def test_vxlan_change_property(vxlan_over_dummy, dummy2):
    for prop_name, prop_value in [
        ("id", 101),
        ("base-iface", "dummy2"),
        ("learning", True),
        ("learning", "true"),
        ("learning", "false"),
        ("learning", False),
        ("destination-port", 4789),
        ("destination-port", "1235"),
        ("ttl", 16),
        ("ttl", "32"),
        ("tos", 24),
        ("tos", "0"),
        ("ageing", 300),
        ("max-address", 512),
        ("proxy", True),
        ("proxy", "false"),
        ("rsc", True),
        ("rsc", "false"),
        ("l2miss", True),
        ("l2miss", "false"),
        ("l3miss", True),
        ("l3miss", "false"),
    ]:
        print(f"Changing VxLAN prop {prop_name} to {prop_value}")
        state = load_yaml(f"""---
                interfaces:
                  - name: {TEST_VXLAN_NIC}
                    type: vxlan
                    state: up
                """)
        state["interfaces"][0]["vxlan"] = {prop_name: prop_value}
        nipart.apply(state)
        vxlan_iface = show_only(TEST_VXLAN_NIC)
        if prop_value == "true":
            prop_value = True
        if prop_value == "false":
            prop_value = False
        if isinstance(prop_value, str) and prop_value.isdigit():
            prop_value = int(prop_value)
        assert vxlan_iface["state"] == "up"
        assert vxlan_iface["vxlan"].get(prop_name) == prop_value
