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


