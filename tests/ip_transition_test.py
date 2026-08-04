# SPDX-License-Identifier: Apache-2.0

import pytest

import nipart

from .testlib.cmdlib import exec_cmd
from .testlib.dhcp import DHCP_SRV_IP4
from .testlib.dhcp import DHCP_SRV_NIC
from .testlib.dhcp import start_dhcp_server
from .testlib.dhcp import stop_dhcp_server
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml
from .testlib.statelib import show_only
from .testlib.statelib import show_saved_only
from .testlib.statelib import state_match
from .testlib.veth import veth_interface

STATIC_TEST_VETH = "veth-ipt0"
STATIC_TEST_VETH_PEER = "veth-ipt1"

DHCP_CLI_NIC = "dhcpcli_t"
TEST_NET_NS = "nipart_dhcp_trans_test"
DEFAULT_TIMEOUT = 20

RA_CLI_NIC = "dhcpcli_ra"
RA_SRV_NIC = "dhcp_srv_ra"
RA_TEST_NET_NS = "nipart_ra_test"
# dnsmasq advertises this prefix via RA; the client gets a SLAAC address
# from it. Keep the static test address outside of it.
RA_PREFIX = "2001:db8:1"
RA_SRV_IP6 = f"{RA_PREFIX}::1"
RA_DNSMASQ_CONF_PATH = "/tmp/nipart_test_ra.conf"
RA_DNSMASQ_PID_PATH = "/tmp/nipart_test_ra.pid"

# The dnsmasq DHCP range in testlib/dhcp.py is 192.0.2.200-192.0.2.250.
DHCP_LEASE_PREFIX = "192.0.2.2"

FIRST_STATIC_IP = "192.0.2.99"
SECOND_STATIC_IP = "192.0.2.100"
SWITCH_STATIC_IP = "198.51.100.7"
STATIC_IPV6 = "2001:db8:2::1"
IPV4_PREFIX_LEN = 24
IPV6_PREFIX_LEN = 64


def _create_veth_pair(ifname, peer, peer_ns):
    exec_cmd(f"ip link add {ifname} type veth peer name {peer}".split())
    exec_cmd(f"ip link set {ifname} up".split())
    exec_cmd(f"ip link set {peer} netns {peer_ns}".split())
    exec_cmd(f"ip netns exec {peer_ns} ip link set {peer} up".split())
    exec_cmd(f"ip netns exec {peer_ns} ip link set lo up".split())


def _remove_veth_pair(ifname, peer_ns):
    exec_cmd(f"ip link del {ifname}".split(), check=False)
    exec_cmd(f"ip netns del {peer_ns}".split(), check=False)


def _ipv4_addrs(iface_state):
    if iface_state is None:
        return []
    return [
        addr.get("ip", "")
        for addr in iface_state.get("ipv4", {}).get("address", [])
    ]


def _ipv6_addrs(iface_state):
    if iface_state is None:
        return []
    return [
        addr.get("ip", "")
        for addr in iface_state.get("ipv6", {}).get("address", [])
    ]


def _has_dhcp_lease():
    return any(
        addr.startswith(DHCP_LEASE_PREFIX)
        for addr in _ipv4_addrs(show_only(DHCP_CLI_NIC))
    )


def _has_ra_addr():
    return any(
        addr.startswith(RA_PREFIX)
        for addr in _ipv6_addrs(show_only(RA_CLI_NIC))
    )


def _ping_dhcp_server():
    try:
        exec_cmd(f"ping {DHCP_SRV_IP4} -c 1 -w 5".split())
        return True
    except Exception:
        return False


@pytest.fixture(scope="module")
def dhcp_env():
    exec_cmd(f"ip netns del {TEST_NET_NS}".split(), check=False)
    exec_cmd(f"ip netns add {TEST_NET_NS}".split())
    exec_cmd(
        f"ip netns exec {TEST_NET_NS} "
        f"sysctl -w net.ipv6.conf.all.disable_ipv6=1".split()
    )
    _create_veth_pair(DHCP_CLI_NIC, DHCP_SRV_NIC, TEST_NET_NS)
    start_dhcp_server(TEST_NET_NS)
    yield
    stop_dhcp_server()
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {DHCP_CLI_NIC}
          type: ethernet
          state: absent"""))
    _remove_veth_pair(DHCP_CLI_NIC, TEST_NET_NS)


@pytest.fixture
def dhcp_cli_cleanup():
    yield
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {DHCP_CLI_NIC}
          type: ethernet
          state: up
          ipv4:
            enabled: false"""))


def _start_ra_server(net_ns):
    dnsmasq_conf = f"""
    leasefile-ro
    interface={RA_SRV_NIC}
    dhcp-range={RA_PREFIX}::,ra-only
    """
    with open(RA_DNSMASQ_CONF_PATH, "w") as fd:
        fd.write(dnsmasq_conf)
    exec_cmd(
        f"sudo ip netns exec {net_ns} dnsmasq "
        f"--interface={RA_SRV_NIC} --enable-ra --log-dhcp "
        f"--pid-file={RA_DNSMASQ_PID_PATH} "
        f"--conf-file={RA_DNSMASQ_CONF_PATH} ".split()
    )


def _stop_ra_server():
    try:
        with open(RA_DNSMASQ_PID_PATH) as fd:
            pid = fd.read().strip()
        if pid:
            exec_cmd(f"kill {pid}".split(), check=False)
    except FileNotFoundError:
        pass


@pytest.fixture(scope="module")
def ra_env():
    exec_cmd(f"ip netns del {RA_TEST_NET_NS}".split(), check=False)
    exec_cmd(f"ip netns add {RA_TEST_NET_NS}".split())
    _create_veth_pair(RA_CLI_NIC, RA_SRV_NIC, RA_TEST_NET_NS)
    exec_cmd(
        f"ip netns exec {RA_TEST_NET_NS} "
        f"sysctl -w net.ipv6.conf.all.disable_ipv6=0".split()
    )
    exec_cmd(
        f"ip netns exec {RA_TEST_NET_NS} "
        f"ip addr add {RA_SRV_IP6}/64 dev {RA_SRV_NIC}".split()
    )
    _start_ra_server(RA_TEST_NET_NS)
    # Pre-seed a SLAAC address on the client (accept_ra defaults to 1 when
    # IPv6 forwarding is disabled), so applying `autoconf: true` later does
    # not race the RA exchange during verification.
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _has_ra_addr)
    yield
    _stop_ra_server()
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {RA_CLI_NIC}
          type: ethernet
          state: absent"""))
    _remove_veth_pair(RA_CLI_NIC, RA_TEST_NET_NS)


@pytest.fixture
def veth_env():
    with veth_interface(STATIC_TEST_VETH, STATIC_TEST_VETH_PEER):
        yield


def test_ipv4_add_second_static_addr_preserves_full_config(veth_env):
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {STATIC_TEST_VETH}
          type: ethernet
          state: up
          ipv4:
            enabled: true
            dhcp: false
            address:
            - ip: {FIRST_STATIC_IP}
              prefix-length: {IPV4_PREFIX_LEN}"""))
    assert retry_till_true_or_timeout(
        DEFAULT_TIMEOUT,
        lambda: _ipv4_addrs(show_only(STATIC_TEST_VETH)) == [FIRST_STATIC_IP],
    )

    # The desired state only adds a second address; `enabled`/`dhcp` come
    # from the saved config.
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {STATIC_TEST_VETH}
          type: ethernet
          state: up
          ipv4:
            dhcp: false
            address:
            - ip: {FIRST_STATIC_IP}
              prefix-length: {IPV4_PREFIX_LEN}
            - ip: {SECOND_STATIC_IP}
              prefix-length: {IPV4_PREFIX_LEN}"""))
    assert retry_till_true_or_timeout(
        DEFAULT_TIMEOUT,
        lambda: sorted(_ipv4_addrs(show_only(STATIC_TEST_VETH)))
        == sorted([FIRST_STATIC_IP, SECOND_STATIC_IP]),
    )

    iface_state = show_only(STATIC_TEST_VETH)
    assert state_match({"enabled": True, "dhcp": False}, iface_state["ipv4"])

    # The saved state must keep the full IPv4 config.
    saved_state = show_saved_only(STATIC_TEST_VETH)
    assert saved_state is not None
    assert state_match({"enabled": True, "dhcp": False}, saved_state["ipv4"])
    assert sorted(_ipv4_addrs(saved_state)) == sorted(
        [FIRST_STATIC_IP, SECOND_STATIC_IP]
    )


def test_ipv4_switch_static_to_dhcp_removes_static_addr(
    dhcp_env, dhcp_cli_cleanup
):
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {DHCP_CLI_NIC}
          type: ethernet
          state: up
          ipv4:
            enabled: true
            dhcp: false
            address:
            - ip: {FIRST_STATIC_IP}
              prefix-length: {IPV4_PREFIX_LEN}"""))
    assert retry_till_true_or_timeout(
        DEFAULT_TIMEOUT,
        lambda: FIRST_STATIC_IP in _ipv4_addrs(show_only(DHCP_CLI_NIC)),
    )

    # Switch to DHCP: the previous static address must be discarded, not
    # applied on top of the DHCP lease.
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {DHCP_CLI_NIC}
          type: ethernet
          state: up
          ipv4:
            enabled: true
            dhcp: true"""))
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _has_dhcp_lease)
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _ping_dhcp_server)
    assert retry_till_true_or_timeout(
        DEFAULT_TIMEOUT,
        lambda: FIRST_STATIC_IP not in _ipv4_addrs(show_only(DHCP_CLI_NIC)),
    )

    # The saved state must not keep the stale static address.
    saved_state = show_saved_only(DHCP_CLI_NIC)
    assert saved_state is not None
    assert state_match({"enabled": True, "dhcp": True}, saved_state["ipv4"])
    assert FIRST_STATIC_IP not in _ipv4_addrs(saved_state)


def test_ipv4_switch_dhcp_to_static_no_addr(dhcp_env, dhcp_cli_cleanup):
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {DHCP_CLI_NIC}
          type: ethernet
          state: up
          ipv4:
            enabled: true
            dhcp: true"""))
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _has_dhcp_lease)

    # Desired state disables DHCP without specifying addresses: no IP at
    # all, the dynamic address must be removed.
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {DHCP_CLI_NIC}
          type: ethernet
          state: up
          ipv4:
            dhcp: false"""))
    assert retry_till_true_or_timeout(
        DEFAULT_TIMEOUT,
        lambda: _ipv4_addrs(show_only(DHCP_CLI_NIC)) == [],
    )

    saved_state = show_saved_only(DHCP_CLI_NIC)
    assert saved_state is not None
    assert state_match({"enabled": True, "dhcp": False}, saved_state["ipv4"])
    assert _ipv4_addrs(saved_state) == []


def test_ipv4_switch_dhcp_to_static_with_addr(dhcp_env, dhcp_cli_cleanup):
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {DHCP_CLI_NIC}
          type: ethernet
          state: up
          ipv4:
            enabled: true
            dhcp: true"""))
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _has_dhcp_lease)

    # Desired state disables DHCP with static addresses specified: the
    # dynamic address must be replaced by the static one.
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {DHCP_CLI_NIC}
          type: ethernet
          state: up
          ipv4:
            dhcp: false
            address:
            - ip: {SWITCH_STATIC_IP}
              prefix-length: {IPV4_PREFIX_LEN}"""))
    assert retry_till_true_or_timeout(
        DEFAULT_TIMEOUT,
        lambda: SWITCH_STATIC_IP in _ipv4_addrs(show_only(DHCP_CLI_NIC))
        and not _has_dhcp_lease(),
    )

    saved_state = show_saved_only(DHCP_CLI_NIC)
    assert saved_state is not None
    assert state_match({"enabled": True, "dhcp": False}, saved_state["ipv4"])
    assert _ipv4_addrs(saved_state) == [SWITCH_STATIC_IP]


@pytest.fixture
def ra_cli_cleanup():
    yield
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {RA_CLI_NIC}
          type: ethernet
          state: up
          ipv6:
            enabled: false"""))


def test_ipv6_switch_static_to_autoconf_removes_static_addr(
    ra_env, ra_cli_cleanup
):
    # ra_env pre-seeds a SLAAC address (see the fixture), so the
    # `autoconf: true` apply below does not race the RA exchange.
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _has_ra_addr)

    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {RA_CLI_NIC}
          type: ethernet
          state: up
          ipv6:
            enabled: true
            address:
            - ip: {STATIC_IPV6}
              prefix-length: {IPV6_PREFIX_LEN}"""))
    assert retry_till_true_or_timeout(
        DEFAULT_TIMEOUT,
        lambda: STATIC_IPV6 in _ipv6_addrs(show_only(RA_CLI_NIC)),
    )

    # Switch to autoconf: the previous static address must be discarded and
    # replaced by the SLAAC address from the RA server. Skip verification
    # here: the apply purges the non-static addresses first and the kernel
    # needs a few seconds to re-acquire the SLAAC address from RA, which
    # races the immediate autoconf verification (a pre-existing behavior).
    nipart.apply(
        load_yaml(f"""---
        interfaces:
        - name: {RA_CLI_NIC}
          type: ethernet
          state: up
          ipv6:
            enabled: true
            autoconf: true"""),
        verify_change=False,
    )
    assert retry_till_true_or_timeout(
        DEFAULT_TIMEOUT,
        lambda: STATIC_IPV6 not in _ipv6_addrs(show_only(RA_CLI_NIC)),
    )
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _has_ra_addr)

    saved_state = show_saved_only(RA_CLI_NIC)
    assert saved_state is not None
    assert state_match({"autoconf": True}, saved_state["ipv6"])
    assert STATIC_IPV6 not in _ipv6_addrs(saved_state)


def test_ipv6_switch_autoconf_to_static_keeps_link_local_only(
    ra_env, ra_cli_cleanup
):
    # ra_env pre-seeds a SLAAC address so this apply does not race the RA
    # exchange during verification.
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _has_ra_addr)

    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {RA_CLI_NIC}
          type: ethernet
          state: up
          ipv6:
            enabled: true
            autoconf: true"""))
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _has_ra_addr)

    # Desired state disables both DHCP and autoconf without specifying
    # addresses: only the kernel-generated link-local address remains. Skip
    # verification: right after the purge the kernel query may still report
    # `autoconf: null` (a pre-existing verification quirk), racing the
    # re-acquisition of the SLAAC address.
    nipart.apply(
        load_yaml(f"""---
        interfaces:
        - name: {RA_CLI_NIC}
          type: ethernet
          state: up
          ipv6:
            enabled: true
            dhcp: false
            autoconf: false"""),
        verify_change=False,
    )
    assert retry_till_true_or_timeout(
        DEFAULT_TIMEOUT,
        lambda: all(
            addr.startswith("fe80:")
            for addr in _ipv6_addrs(show_only(RA_CLI_NIC))
        ),
    )

    saved_state = show_saved_only(RA_CLI_NIC)
    assert saved_state is not None
    assert state_match({"dhcp": False, "autoconf": False}, saved_state["ipv6"])
    assert _ipv6_addrs(saved_state) == []
