# SPDX-License-Identifier: Apache-2.0

from operator import itemgetter

import pytest

import nipart

from .testlib.statelib import load_yaml
from .testlib.statelib import show_only

TEST_BASE_NIC = "dummy1"
TEST_BASE_NIC2 = "dummy2"
TEST_VLAN_NIC = "dummy1.100"


@pytest.fixture
def vlan_over_dummy():
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_VLAN_NIC}
                type: vlan
                state: up
                vlan:
                  id: 100
                  base-iface: {TEST_BASE_NIC}
              - name: {TEST_BASE_NIC}
                type: dummy
                state: up
            """))
    yield
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_VLAN_NIC}
                type: vlan
                state: absent
              - name: {TEST_BASE_NIC}
                type: dummy
                state: absent
            """))


def test_create_and_remove_vlan(vlan_over_dummy):
    vlan_iface = show_only(TEST_VLAN_NIC)
    assert vlan_iface["vlan"]["id"] == 100
    assert vlan_iface["vlan"]["base-iface"] == TEST_BASE_NIC


@pytest.fixture
def dummy2():
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_BASE_NIC2}
                type: dummy
                state: up
            """))
    yield
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_BASE_NIC2}
                type: dummy
                state: absent
            """))


