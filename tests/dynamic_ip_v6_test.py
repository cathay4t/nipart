# SPDX-License-Identifier: Apache-2.0

import pytest

import nipart

from .testlib.cmdlib import exec_cmd
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml
from .testlib.statelib import show_only

DHCPV6_CLI_NIC = "dhcpcli_v6"
DHCPV6_SRV_NIC = "dhcp_srv_v6"
TEST_NET_NS = "nipart_dhcp_v6_test"
DEFAULT_TIMEOUT = 40

# DHCPv6 lease provides /128 address only, no routes.
DHCPV6_PREFIX = "2001:db8:2"
DHCPV6_SRV_IP6 = f"{DHCPV6_PREFIX}::1"
DHCPV6_LEASE_PREFIX_LEN = 128

DNSMASQ_CONF_PATH = "/tmp/nipart_test_dnsmasq_v6.conf"
DNSMASQ_PID_PATH = "/tmp/nipart_test_dnsmasq_v6.pid"


def _enable_ipv6(net_ns):
    exec_cmd(
        f"ip netns exec {net_ns} "
        f"sysctl -w net.ipv6.conf.all.disable_ipv6=0".split()
    )


def _create_veth_pair(ifname, peer, peer_ns):
    exec_cmd(f"ip link add {ifname} type veth peer name {peer}".split())
    exec_cmd(f"ip link set {ifname} up".split())
    exec_cmd(f"ip link set {peer} netns {peer_ns}".split())
    exec_cmd(f"ip netns exec {peer_ns} ip link set {peer} up".split())
    exec_cmd(f"ip netns exec {peer_ns} ip link set lo up".split())
    # Simulate the interface with IPv6 previously disabled. nipart should
    # enable IPv6 (disable_ipv6=0) before starting DHCPv6.
    exec_cmd(f"sysctl -w net.ipv6.conf.{ifname}.disable_ipv6=1".split())
    exec_cmd(
        f"ip netns exec {peer_ns} "
        f"sysctl -w net.ipv6.conf.{peer}.disable_ipv6=0".split()
    )


def _remove_veth_pair(ifname, peer_ns):
    exec_cmd(f"ip link del {ifname}".split(), check=False)
    exec_cmd(f"ip netns del {peer_ns}".split(), check=False)


def _start_dhcpv6_server(net_ns):
    exec_cmd(
        f"ip netns exec {net_ns} "
        f"ip addr add {DHCPV6_SRV_IP6}/64 dev {DHCPV6_SRV_NIC}".split()
    )
    dnsmasq_conf = f"""
    leasefile-ro
    interface={DHCPV6_SRV_NIC}
    dhcp-range={DHCPV6_PREFIX}::100,{DHCPV6_PREFIX}::200,12h
    """
    with open(DNSMASQ_CONF_PATH, "w") as fd:
        fd.write(dnsmasq_conf)

    exec_cmd(
        f"sudo ip netns exec {net_ns} dnsmasq "
        f"--interface={DHCPV6_SRV_NIC} --log-dhcp "
        f"--pid-file={DNSMASQ_PID_PATH} "
        f"--conf-file={DNSMASQ_CONF_PATH} ".split()
    )


def _stop_dhcpv6_server():
    try:
        with open(DNSMASQ_PID_PATH, "r") as fd:
            pid = fd.read().strip()
        if pid:
            exec_cmd(f"kill {pid}".split(), check=False)
    except FileNotFoundError:
        pass


@pytest.fixture(scope="module")
def dhcpv6_env():
    exec_cmd(f"ip netns del {TEST_NET_NS}".split(), check=False)
    exec_cmd(f"ip netns add {TEST_NET_NS}".split())
    _enable_ipv6(TEST_NET_NS)
    _create_veth_pair(DHCPV6_CLI_NIC, DHCPV6_SRV_NIC, TEST_NET_NS)
    _start_dhcpv6_server(TEST_NET_NS)
    yield
    _stop_dhcpv6_server()
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {DHCPV6_CLI_NIC}
          type: ethernet
          state: absent"""))
    _remove_veth_pair(DHCPV6_CLI_NIC, TEST_NET_NS)


@pytest.fixture
def dhcpv6_cli_cleanup():
    yield
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {DHCPV6_CLI_NIC}
          type: ethernet
          state: up
          ipv6:
            enabled: false"""))


def _get_ipv6_routes(nic=DHCPV6_CLI_NIC):
    rc, out, _ = exec_cmd(["ip", "-6", "route", "show", "dev", nic])
    return out


def _has_default_route():
    routes = _get_ipv6_routes()
    for line in routes.splitlines():
        if line.lstrip().startswith("default"):
            return True
    return False


def _has_dhcpv6_addr():
    iface_state = show_only(DHCPV6_CLI_NIC)
    if iface_state is None:
        return False
    addrs = iface_state.get("ipv6", {}).get("address", [])
    for addr in addrs:
        if (
            DHCPV6_PREFIX in addr.get("ip", "")
            and addr.get("prefix-length") == DHCPV6_LEASE_PREFIX_LEN
        ):
            return True
    return False


def _dhcpv6_state_done():
    iface_state = show_only(DHCPV6_CLI_NIC)
    if iface_state is None:
        return False
    ipv6_conf = iface_state.get("ipv6", {})
    return ipv6_conf.get("dhcp") is True and (
        ipv6_conf.get("dhcp-state") == "done"
    )


def _ipv6_enabled_on_iface():
    rc, out, _ = exec_cmd(
        [
            "sysctl",
            "-n",
            f"net.ipv6.conf.{DHCPV6_CLI_NIC}.disable_ipv6",
        ]
    )
    return out.strip() == "0"


def _has_link_local(nic=DHCPV6_CLI_NIC):
    rc, out, _ = exec_cmd(["ip", "-6", "addr", "show", "dev", nic])
    return "fe80::" in out


def _del_link_local(nic=DHCPV6_CLI_NIC):
    rc, out, _ = exec_cmd(["ip", "-6", "addr", "show", "dev", nic])
    for line in out.splitlines():
        if "fe80::" in line:
            addr = line.strip().split()[1]
            exec_cmd(f"ip -6 addr del {addr} dev {nic}".split())
            return


def test_dhcpv6_regenerates_link_local(dhcpv6_env, dhcpv6_cli_cleanup):
    # Simulate an interface holding no IPv6 link-local address (e.g. deleted
    # externally). nipart should flip disable_ipv6 so the kernel regenerates
    # the link-local address before starting DHCPv6.
    exec_cmd(
        f"sysctl -w net.ipv6.conf.{DHCPV6_CLI_NIC}.disable_ipv6=0".split()
    )
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _has_link_local)
    _del_link_local()
    assert not _has_link_local()
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {DHCPV6_CLI_NIC}
          type: ethernet
          state: up
          ipv6:
            enabled: true
            dhcp: true"""))
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _has_link_local)
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _has_dhcpv6_addr)
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _dhcpv6_state_done)
    assert not _has_default_route()


# IPv6 autoconf (SLAAC) test settings.
AUTOCONF_CLI_NIC = "racli"
AUTOCONF_SRV_NIC = "rasrv"
AUTOCONF_TEST_NET_NS = "nipart_autoconf_v6_test"
AUTOCONF_PREFIX = "2001:db8:3"
AUTOCONF_SRV_IP6 = f"{AUTOCONF_PREFIX}::1"
AUTOCONF_DNSMASQ_CONF_PATH = "/tmp/nipart_test_dnsmasq_ra.conf"
AUTOCONF_DNSMASQ_PID_PATH = "/tmp/nipart_test_dnsmasq_ra.pid"


def _start_ra_server(net_ns):
    exec_cmd(
        f"ip netns exec {net_ns} "
        f"ip addr add {AUTOCONF_SRV_IP6}/64 dev {AUTOCONF_SRV_NIC}".split()
    )
    dnsmasq_conf = f"""
    leasefile-ro
    interface={AUTOCONF_SRV_NIC}
    dhcp-range={AUTOCONF_PREFIX}::,ra-only
    """
    with open(AUTOCONF_DNSMASQ_CONF_PATH, "w") as fd:
        fd.write(dnsmasq_conf)

    exec_cmd(
        f"sudo ip netns exec {net_ns} dnsmasq "
        f"--interface={AUTOCONF_SRV_NIC} --enable-ra --log-dhcp "
        f"--pid-file={AUTOCONF_DNSMASQ_PID_PATH} "
        f"--conf-file={AUTOCONF_DNSMASQ_CONF_PATH} ".split()
    )


def _stop_ra_server():
    try:
        with open(AUTOCONF_DNSMASQ_PID_PATH, "r") as fd:
            pid = fd.read().strip()
        if pid:
            exec_cmd(f"kill {pid}".split(), check=False)
    except FileNotFoundError:
        pass


@pytest.fixture(scope="module")
def autoconf_env():
    exec_cmd(f"ip netns del {AUTOCONF_TEST_NET_NS}".split(), check=False)
    exec_cmd(f"ip netns add {AUTOCONF_TEST_NET_NS}".split())
    _enable_ipv6(AUTOCONF_TEST_NET_NS)
    _create_veth_pair(AUTOCONF_CLI_NIC, AUTOCONF_SRV_NIC, AUTOCONF_TEST_NET_NS)
    _start_ra_server(AUTOCONF_TEST_NET_NS)
    yield
    _stop_ra_server()
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {AUTOCONF_CLI_NIC}
          type: ethernet
          state: absent"""))
    _remove_veth_pair(AUTOCONF_CLI_NIC, AUTOCONF_TEST_NET_NS)


@pytest.fixture
def autoconf_cli_cleanup():
    yield
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {AUTOCONF_CLI_NIC}
          type: ethernet
          state: up
          ipv6:
            enabled: false"""))


def _accept_ra_forced():
    rc, out, _ = exec_cmd(
        ["sysctl", "-n", f"net.ipv6.conf.{AUTOCONF_CLI_NIC}.accept_ra"]
    )
    return out.strip() == "2"


def _has_autoconf_addr():
    iface_state = show_only(AUTOCONF_CLI_NIC)
    if iface_state is None:
        return False
    addrs = iface_state.get("ipv6", {}).get("address", [])
    for addr in addrs:
        if AUTOCONF_PREFIX in addr.get("ip", ""):
            return True
    return False


def test_ipv6_autoconf_forces_accept_ra(autoconf_env, autoconf_cli_cleanup):
    # Let the kernel acquire a SLAAC address first (accept_ra defaults to 1
    # when IPv6 forwarding is disabled), so that verifying autoconf: true
    # does not race the RA exchange during apply.
    exec_cmd(
        f"sysctl -w net.ipv6.conf.{AUTOCONF_CLI_NIC}.disable_ipv6=0".split()
    )
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _has_autoconf_addr)
    assert not _accept_ra_forced()
    # When autoconf is enabled, nipart should change
    # /proc/sys/net/ipv6/conf/<iface>/accept_ra to 2 to force allow
    # IPv6-RA (SLAAC) to run.
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {AUTOCONF_CLI_NIC}
          type: ethernet
          state: up
          ipv6:
            enabled: true
            autoconf: true"""))
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _accept_ra_forced)
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _has_autoconf_addr)


def test_dhcpv6_lease_no_route(dhcpv6_env, dhcpv6_cli_cleanup):
    # DHCPv6 only provides a /128 address, no default route.
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {DHCPV6_CLI_NIC}
          type: ethernet
          state: up
          ipv6:
            enabled: true
            dhcp: true"""))
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _has_dhcpv6_addr)
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _dhcpv6_state_done)
    assert not _has_default_route()


def test_dhcpv6_enables_ipv6_when_previously_disabled(
    dhcpv6_env, dhcpv6_cli_cleanup
):
    # The client interface starts with IPv6 disabled, nipart should change
    # /proc/sys/net/ipv6/conf/<iface>/disable_ipv6 to 0 before starting
    # DHCPv6. Re-disable it here in case a previous test enabled it.
    exec_cmd(
        f"sysctl -w net.ipv6.conf.{DHCPV6_CLI_NIC}.disable_ipv6=1".split()
    )
    nipart.apply(load_yaml(f"""---
        interfaces:
        - name: {DHCPV6_CLI_NIC}
          type: ethernet
          state: up
          ipv6:
            enabled: true
            dhcp: true"""))
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _ipv6_enabled_on_iface)
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _has_dhcpv6_addr)
    assert retry_till_true_or_timeout(DEFAULT_TIMEOUT, _dhcpv6_state_done)
    assert not _has_default_route()
