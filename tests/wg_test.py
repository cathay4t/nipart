# SPDX-License-Identifier: Apache-2.0

import pytest

import nipart

from .testlib.cmdlib import exec_cmd
from .testlib.statelib import load_yaml

WG_TEST_NIC = "wg0"
NEW_WG_TEST_NIC = "wg1"

WG0_IP = "192.0.2.3"
WG1_IP = "192.0.2.4"
# On-link gateway shared by both wireguard interfaces (192.0.2.0/24).
WG_GATEWAY = "192.0.2.1"
# Routes stored by the first apply; they must survive the second apply.
WG0_ROUTE_DESTS = ("198.51.100.0/25", "203.0.113.0/25")
# Routes added together with the new wireguard interface.
WG1_ROUTE_DESTS = ("198.51.100.128/25", "203.0.113.128/25")


@pytest.fixture
def clean_up():
    yield
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {WG_TEST_NIC}
                type: wireguard
                state: absent"""))


@pytest.fixture
def two_wg_clean_up():
    yield
    nipart.apply(load_yaml("""---
        version: 1
        interfaces:
          - name: wg0
            type: wireguard
            state: absent
          - name: wg1
            type: wireguard
            state: absent"""))


def _wg_apply_state(iface_name, ip, listen_port, via_route_dst, dev_route_dst):
    """Desired state creating a wireguard interface with two static routes:
    one via a gateway and one dev-only (no next-hop-address)."""
    return f"""---
        version: 1
        interfaces:
          - name: {iface_name}
            type: wireguard
            state: up
            ipv4:
              enabled: true
              dhcp: false
              address:
                - ip: {ip}
                  prefix-length: 24
            wireguard:
              public-key: "JKossUAjywXuJ2YVcaeD6PaHs+afPmIthDuqEVlspwA="
              private-key: "6LTHiAM4vgKEgi5vm30f/EBIEWFDmySkTc9EWCcIqEs="
              listen-port: {listen_port}
              peers:
                - endpoint: 192.0.2.0:51820
                  public-key: 8bdQrVLqiw3ZoHCucNh1YfH0iCWuyStniRr8t7H24Fk=
                  preshared-key: TqIkTsTSxWJ1vSnhUW2oXFAtB5l9hRFWdgn2BrKX3ik=
                  persistent-keepalive: 0
                  allowed-ips:
                  - ip: 0.0.0.0
                    prefix-length: 0
                  - ip: '::'
                    prefix-length: 0
                  protocol-version: 1
        routes:
          config:
          - destination: {via_route_dst}
            next-hop-interface: {iface_name}
            next-hop-address: {WG_GATEWAY}
            metric: 100
            table-id: 254
          - destination: {dev_route_dst}
            next-hop-interface: {iface_name}
            metric: 100
            table-id: 254
    """


def _saved_route_dests(iface_name):
    saved_state = nipart.NipartClient().query_network_state(
        nipart.NipartQueryOption.saved()
    )
    return {
        route["destination"]
        for route in saved_state.get("routes", {}).get("config", [])
        if route.get("next-hop-interface") == iface_name
    }


def _kernel_route_dests(iface_name):
    _, out, _ = exec_cmd(
        ["ip", "route", "show", "dev", iface_name], check=False
    )
    return {line.split()[0] for line in out.splitlines() if line.split()}


def test_wireguard_iface_static_ip(clean_up):
    desired_state = load_yaml(f"""---
        interfaces:
          - name: {WG_TEST_NIC}
            type: wireguard
            state: up
            wireguard:
              public-key: "JKossUAjywXuJ2YVcaeD6PaHs+afPmIthDuqEVlspwA="
              private-key: "6LTHiAM4vgKEgi5vm30f/EBIEWFDmySkTc9EWCcIqEs="
              listen-port: 51820
              peers:
                - endpoint: 192.0.2.0:51820
                  public-key: 8bdQrVLqiw3ZoHCucNh1YfH0iCWuyStniRr8t7H24Fk=
                  preshared-key: TqIkTsTSxWJ1vSnhUW2oXFAtB5l9hRFWdgn2BrKX3ik=
                  persistent-keepalive: 0
                  allowed-ips:
                  - ip: 0.0.0.0
                    prefix-length: 0
                  - ip: '::'
                    prefix-length: 0
                  protocol-version: 1
        """)
    nipart.apply(desired_state)


def test_new_wg_iface_with_routes_keeps_existing_saved_routes(two_wg_clean_up):
    # First apply: create the wireguard interface wg0 together with its
    # static routes, which then become part of the stored (saved) state.
    nipart.apply(
        load_yaml(
            _wg_apply_state(WG_TEST_NIC, WG0_IP, 51820, *WG0_ROUTE_DESTS)
        )
    )

    saved_dests = _saved_route_dests(WG_TEST_NIC)
    assert set(WG0_ROUTE_DESTS) <= saved_dests, (
        f"Expected {WG0_ROUTE_DESTS} stored via {WG_TEST_NIC} after first "
        f"apply, got {saved_dests}"
    )

    # Second apply: add a NEW wireguard interface wg1 with its own static
    # routes, without mentioning wg0 or its routes at all. The previously
    # stored routes of wg0 must not be overridden by this partial apply.
    nipart.apply(
        load_yaml(
            _wg_apply_state(NEW_WG_TEST_NIC, WG1_IP, 51821, *WG1_ROUTE_DESTS)
        )
    )

    saved_dests = _saved_route_dests(WG_TEST_NIC)
    assert set(WG0_ROUTE_DESTS) <= saved_dests, (
        f"Stored routes of {WG_TEST_NIC} were overridden by adding "
        f"{NEW_WG_TEST_NIC}: got {saved_dests}"
    )
    assert set(WG1_ROUTE_DESTS) <= _saved_route_dests(
        NEW_WG_TEST_NIC
    ), f"Expected {WG1_ROUTE_DESTS} stored via {NEW_WG_TEST_NIC}"

    # The kernel must still carry the original routes of wg0 as well.
    kernel_dests = _kernel_route_dests(WG_TEST_NIC)
    assert set(WG0_ROUTE_DESTS) <= kernel_dests, (
        f"Kernel routes of {WG_TEST_NIC} were dropped by adding "
        f"{NEW_WG_TEST_NIC}: got {kernel_dests}"
    )
    assert set(WG1_ROUTE_DESTS) <= _kernel_route_dests(
        NEW_WG_TEST_NIC
    ), f"Expected {WG1_ROUTE_DESTS} in kernel via {NEW_WG_TEST_NIC}"
