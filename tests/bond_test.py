# SPDX-License-Identifier: Apache-2.0

import pytest

import nipart

from .testlib.env import has_kernel_module
from .testlib.statelib import load_yaml
from .testlib.statelib import show_only
from .testlib.statelib import show_saved_only
from .testlib.statelib import state_match
from .testlib.veth import veth_interface

TEST_PORT1 = "dummy1"
TEST_PORT2 = "dummy2"
TEST_BOND_NIC = "bond99"
TEST_VETH0 = "veth0"
TEST_VETH1 = "veth1"
TEST_PORT_NAME0 = "bond-port0"
TEST_PORT_NAME1 = "bond-port1"


@pytest.fixture
def bond_over_dummy():
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_BOND_NIC}
                type: bond
                state: up
                bond:
                  mode: active-backup
                  ports:
                  - name: {TEST_PORT1}
                    queue-id: 1
                    priority: 1
                  - name: {TEST_PORT2}
                    queue-id: 2
                    priority: 2
              - name: {TEST_PORT1}
                type: dummy
                state: up
              - name: {TEST_PORT2}
                type: dummy
                state: up
            """))
    yield
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_BOND_NIC}
                type: bond
                state: absent
              - name: {TEST_PORT1}
                type: dummy
                state: absent
              - name: {TEST_PORT2}
                type: dummy
                state: absent
            """))


@pytest.fixture
def bond_over_veth_mac():
    with veth_interface(TEST_VETH0, TEST_VETH1):
        iface0 = show_only(TEST_VETH0)
        mac0 = iface0["mac-address"]
        iface1 = show_only(TEST_VETH1)
        mac1 = iface1["mac-address"]

        nipart.apply(load_yaml(f"""---
                interfaces:
                  - name: {TEST_BOND_NIC}
                    type: bond
                    state: up
                    bond:
                      mode: active-backup
                      ports:
                      - name: {TEST_PORT_NAME0}
                      - name: {TEST_PORT_NAME1}
                  - name: {TEST_PORT_NAME0}
                    type: ethernet
                    mac-address: {mac0}
                    identifier: mac-address
                  - name: {TEST_PORT_NAME1}
                    type: ethernet
                    mac-address: {mac1}
                    identifier: mac-address
                """))

        yield

        nipart.apply(load_yaml(f"""---
                interfaces:
                  - name: {TEST_BOND_NIC}
                    type: bond
                    state: absent
                  - name: {TEST_PORT_NAME0}
                    type: veth
                    state: absent
                  - name: {TEST_PORT_NAME1}
                    type: veth
                    state: absent
                """))


def test_create_and_remove_bond(bond_over_dummy):
    bond_iface = show_only(TEST_BOND_NIC)
    assert bond_iface["bond"]["mode"] == "active-backup"
    assert state_match(
        [{"name": TEST_PORT1}, {"name": TEST_PORT2}],
        bond_iface["bond"]["ports"],
    )


def test_bond_change_mode(bond_over_dummy):
    state = load_yaml(f"""---
        interfaces:
          - name: {TEST_BOND_NIC}
            type: bond
            state: up
            bond:
              mode: 0
        """)
    nipart.apply(state)
    bond_iface = show_only(TEST_BOND_NIC)
    assert bond_iface["state"] == "up"
    assert bond_iface["bond"]["mode"] == "balance-rr"
    assert state_match(
        [{"name": TEST_PORT1}, {"name": TEST_PORT2}],
        bond_iface["bond"]["ports"],
    )


def test_bond_change_port_config(bond_over_dummy):
    state = load_yaml(f"""---
        interfaces:
          - name: {TEST_BOND_NIC}
            type: bond
            state: up
            bond:
              ports:
              - name: {TEST_PORT1}
                queue-id: 0
                priority: 10
              - name: {TEST_PORT2}
                queue-id: 0
                priority: 20
        """)
    nipart.apply(state)
    bond_iface = show_only(TEST_BOND_NIC)
    assert bond_iface["state"] == "up"
    assert state_match(
        [
            {
                "name": TEST_PORT1,
                "queue-id": 0,
                "priority": 10,
            },
            {
                "name": TEST_PORT2,
                "queue-id": 0,
                "priority": 20,
            },
        ],
        bond_iface["bond"]["ports"],
    )


def test_bond_port_ref_by_mac_identifier(bond_over_veth_mac):
    bond_iface = show_only(TEST_BOND_NIC)
    assert bond_iface["state"] == "up"
    assert bond_iface["bond"]["mode"] == "active-backup"
    assert state_match(
        [{"name": TEST_VETH0}, {"name": TEST_VETH1}],
        bond_iface["bond"]["ports"],
    )


def test_bond_saved_state_keeps_profile_names(bond_over_veth_mac):
    saved_iface = show_saved_only(TEST_BOND_NIC)
    assert state_match(
        [{"name": TEST_PORT_NAME0}, {"name": TEST_PORT_NAME1}],
        saved_iface["bond"]["ports"],
    )


def test_bond_mac_identifier_port_reapply(bond_over_veth_mac):
    # The bond already exists and controls the port MACs (active-backup
    # assigns the bond's MAC to every slave). The running state reports
    # the ports' original MACs (kept in BondPortInfo by the bond driver)
    # as `permanent-mac-address`; re-applying the same MAC-identifier
    # bond state with those must succeed: the MAC is only an identifier
    # for locating the port, it must not be verified against the
    # bond-assigned port MAC.
    port0 = show_only(TEST_VETH0)
    port1 = show_only(TEST_VETH1)
    mac0 = port0["permanent-mac-address"]
    mac1 = port1["permanent-mac-address"]

    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_BOND_NIC}
                type: bond
                state: up
                bond:
                  mode: active-backup
                  ports:
                  - name: {TEST_PORT_NAME0}
                  - name: {TEST_PORT_NAME1}
              - name: {TEST_PORT_NAME0}
                type: ethernet
                identifier: mac-address
                mac-address: {mac0}
              - name: {TEST_PORT_NAME1}
                type: ethernet
                identifier: mac-address
                mac-address: {mac1}
            """))

    bond_iface = show_only(TEST_BOND_NIC)
    assert bond_iface["state"] == "up"
    assert bond_iface["bond"]["mode"] == "active-backup"
    assert state_match(
        [{"name": TEST_VETH0}, {"name": TEST_VETH1}],
        bond_iface["bond"]["ports"],
    )


def test_bond_mac_identifier_port_order_swapped(bond_over_veth_mac):
    # Swapping the MAC addresses of the two `identifier: mac-address`
    # ports resolves the desired bond port order to [veth1, veth0] while
    # the current bond holds [veth0, veth1]. The port set is unchanged,
    # so the apply must succeed without failing verification on the port
    # order (the bond ports are sorted before comparison).
    mac0 = show_only(TEST_VETH0)["permanent-mac-address"]
    mac1 = show_only(TEST_VETH1)["permanent-mac-address"]

    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_BOND_NIC}
                type: bond
                state: up
                bond:
                  mode: active-backup
                  ports:
                  - name: {TEST_PORT_NAME0}
                  - name: {TEST_PORT_NAME1}
              - name: {TEST_PORT_NAME0}
                type: ethernet
                identifier: mac-address
                mac-address: {mac1}
              - name: {TEST_PORT_NAME1}
                type: ethernet
                identifier: mac-address
                mac-address: {mac0}
            """))

    bond_iface = show_only(TEST_BOND_NIC)
    assert bond_iface["state"] == "up"
    assert bond_iface["bond"]["mode"] == "active-backup"
    # The port set is unchanged, the bond keeps its original port order.
    assert state_match(
        [{"name": TEST_VETH0}, {"name": TEST_VETH1}],
        bond_iface["bond"]["ports"],
    )
