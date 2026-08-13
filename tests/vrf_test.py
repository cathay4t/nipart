# SPDX-License-Identifier: Apache-2.0

import pytest

import nipart

from .conftest import start_daemon
from .conftest import stop_daemon
from .testlib.cmdlib import exec_cmd
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml
from .testlib.statelib import show_only
from .testlib.statelib import show_saved_only
from .testlib.veth import veth_interface

TEST_VRF0 = "test-vrf0"
TEST_PORT0 = "dummy1"
TEST_PORT1 = "dummy2"
TEST_VETH0 = "veth0"
TEST_VETH1 = "veth1"
TEST_PORT_NAME0 = "vrf-port0"
TEST_PORT_NAME1 = "vrf-port1"
TEST_ROUTE_TABLE_ID0 = 100
TEST_ROUTE_TABLE_ID1 = 101
TEST_ROUTE_DST = "198.51.100.0/24"


@pytest.fixture
def vrf0_over_dummy():
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_VRF0}
                type: vrf
                state: up
                vrf:
                  route-table-id: {TEST_ROUTE_TABLE_ID0}
                  ports:
                    - {TEST_PORT0}
                    - {TEST_PORT1}
              - name: {TEST_PORT0}
                type: dummy
                state: up
              - name: {TEST_PORT1}
                type: dummy
                state: up
            """))
    yield
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_VRF0}
                type: vrf
                state: absent
              - name: {TEST_PORT0}
                type: dummy
                state: absent
              - name: {TEST_PORT1}
                type: dummy
                state: absent
            """))


def test_create_and_remove_vrf(vrf0_over_dummy):
    vrf_iface = show_only(TEST_VRF0)
    assert vrf_iface["type"] == "vrf"
    assert vrf_iface["state"] == "up"
    assert vrf_iface["vrf"]["route-table-id"] == TEST_ROUTE_TABLE_ID0
    assert vrf_iface["vrf"]["ports"] == [TEST_PORT0, TEST_PORT1]

    # Ports should be enslaved by the VRF interface.
    port0 = show_only(TEST_PORT0)
    assert port0["controller"] == TEST_VRF0
    port1 = show_only(TEST_PORT1)
    assert port1["controller"] == TEST_VRF0


def test_vrf_change_route_table_id(vrf0_over_dummy):
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_VRF0}
                type: vrf
                state: up
                vrf:
                  route-table-id: {TEST_ROUTE_TABLE_ID1}
            """))
    vrf_iface = show_only(TEST_VRF0)
    assert vrf_iface["vrf"]["route-table-id"] == TEST_ROUTE_TABLE_ID1
    # Ports should be preserved and re-attached after table ID change.
    assert vrf_iface["vrf"]["ports"] == [TEST_PORT0, TEST_PORT1]
    port0 = show_only(TEST_PORT0)
    assert port0["controller"] == TEST_VRF0


def test_vrf_add_and_remove_port(vrf0_over_dummy):
    # Remove dummy2 from the VRF port list.
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_VRF0}
                type: vrf
                state: up
                vrf:
                  ports:
                    - {TEST_PORT0}
            """))
    vrf_iface = show_only(TEST_VRF0)
    assert vrf_iface["vrf"]["ports"] == [TEST_PORT0]
    port1 = show_only(TEST_PORT1)
    assert port1.get("controller") is None

    # Add dummy2 back to the VRF port list.
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_VRF0}
                type: vrf
                state: up
                vrf:
                  ports:
                    - {TEST_PORT0}
                    - {TEST_PORT1}
            """))
    vrf_iface = show_only(TEST_VRF0)
    assert vrf_iface["vrf"]["ports"] == [TEST_PORT0, TEST_PORT1]
    port1 = show_only(TEST_PORT1)
    assert port1["controller"] == TEST_VRF0


def test_vrf_ignore_mac_address(vrf0_over_dummy):
    # MAC address should be ignored for layer 3 VRF interface.
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_VRF0}
                type: vrf
                state: up
                vrf:
                  route-table-id: {TEST_ROUTE_TABLE_ID0}
                mac-address: 02:00:00:00:00:0e
            """))
    vrf_iface = show_only(TEST_VRF0)
    assert vrf_iface["vrf"]["route-table-id"] == TEST_ROUTE_TABLE_ID0


def test_vrf_route_by_vrf_name(vrf0_over_dummy):
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_PORT0}
                type: dummy
                state: up
                ipv4:
                  enabled: true
                  dhcp: false
                  address:
                    - ip: 192.0.2.1
                      prefix-length: 24
            routes:
              config:
                - destination: {TEST_ROUTE_DST}
                  next-hop-interface: {TEST_PORT0}
                  metric: 100
                  vrf-name: {TEST_VRF0}
            """))
    rc, out, _ = exec_cmd(
        ["ip", "route", "show", "table", str(TEST_ROUTE_TABLE_ID0)],
        check=False,
    )
    assert TEST_ROUTE_DST in out
    # The route should NOT be in the main table.
    rc, out, _ = exec_cmd(
        ["ip", "route", "show", "table", "main"], check=False
    )
    assert TEST_ROUTE_DST not in out


def test_vrf_create_without_table_id_fails():
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_PORT0}
                type: dummy
                state: up
            """))
    with pytest.raises(Exception):
        nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_VRF0}
                type: vrf
                state: up
                vrf:
                  ports:
                    - {TEST_PORT0}
            """))
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_PORT0}
                type: dummy
                state: absent
            """))


def test_vrf_legacy_port_alias():
    # `port` is a deprecated alias of `ports` (nmstate-style schema), it
    # must still be accepted and the query/saved config uses `ports`.
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_VRF0}
                type: vrf
                state: up
                vrf:
                  route-table-id: {TEST_ROUTE_TABLE_ID0}
                  port:
                    - {TEST_PORT0}
              - name: {TEST_PORT0}
                type: dummy
                state: up
            """))
    try:
        vrf_iface = show_only(TEST_VRF0)
        assert vrf_iface["vrf"]["ports"] == [TEST_PORT0]
    finally:
        nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_VRF0}
                type: vrf
                state: absent
              - name: {TEST_PORT0}
                type: dummy
                state: absent
            """))


def test_vrf_saved_config_restored_after_daemon_restart(vrf0_over_dummy):
    # The fixture applied the VRF (which also saved its config to
    # /etc/nipart/states). Restart the daemon so it restores the whole VRF
    # (table ID + ports) from the saved config.
    stop_daemon()
    start_daemon()

    def _vrf_restored():
        vrf_iface = show_only(TEST_VRF0)
        if vrf_iface is None:
            return False
        if (
            vrf_iface.get("vrf", {}).get("route-table-id")
            != TEST_ROUTE_TABLE_ID0
        ):
            return False
        if vrf_iface.get("vrf", {}).get("ports") != [TEST_PORT0, TEST_PORT1]:
            return False
        port0 = show_only(TEST_PORT0)
        return port0 is not None and port0.get("controller") == TEST_VRF0

    assert retry_till_true_or_timeout(30, _vrf_restored), (
        "VRF not restored from saved config after daemon restart"
    )


def test_vrf_port_ref_by_mac_identifier():
    # The VRF port list references the ports by their profile names, while
    # the ports themselves are located by `identifier: mac-address` (like
    # nmstate's `interfaces-mac` profile reference).
    with veth_interface(TEST_VETH0, TEST_VETH1):
        iface0 = show_only(TEST_VETH0)
        mac0 = iface0["mac-address"]
        iface1 = show_only(TEST_VETH1)
        mac1 = iface1["mac-address"]

        nipart.apply(load_yaml(f"""---
                interfaces:
                  - name: {TEST_VRF0}
                    type: vrf
                    state: up
                    vrf:
                      route-table-id: {TEST_ROUTE_TABLE_ID0}
                      ports:
                      - {TEST_PORT_NAME0}
                      - {TEST_PORT_NAME1}
                  - name: {TEST_PORT_NAME0}
                    type: ethernet
                    identifier: mac-address
                    mac-address: {mac0}
                  - name: {TEST_PORT_NAME1}
                    type: ethernet
                    identifier: mac-address
                    mac-address: {mac1}
                """))
        try:
            # The query reports the resolved kernel interface names.
            vrf_iface = show_only(TEST_VRF0)
            assert vrf_iface["vrf"]["route-table-id"] == TEST_ROUTE_TABLE_ID0
            assert vrf_iface["vrf"]["ports"] == [TEST_VETH0, TEST_VETH1]

            # Both ports should be enslaved by the VRF.
            port0 = show_only(TEST_VETH0)
            assert port0["controller"] == TEST_VRF0
            port1 = show_only(TEST_VETH1)
            assert port1["controller"] == TEST_VRF0

            # The saved config keeps the profile names, so the VRF can be
            # restored by MAC identifier after a daemon restart.
            saved_iface = show_saved_only(TEST_VRF0)
            assert saved_iface["vrf"]["ports"] == [
                TEST_PORT_NAME0,
                TEST_PORT_NAME1,
            ]
        finally:
            nipart.apply(load_yaml(f"""---
                interfaces:
                  - name: {TEST_VRF0}
                    type: vrf
                    state: absent
                  - name: {TEST_PORT_NAME0}
                    type: veth
                    state: absent
                  - name: {TEST_PORT_NAME1}
                    type: veth
                    state: absent
                """))
