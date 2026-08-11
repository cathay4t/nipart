# SPDX-License-Identifier: Apache-2.0

import os

import nipart

from .conftest import DAEMON_LOG, start_daemon, stop_daemon
from .testlib.cmdlib import exec_cmd
from .testlib.dhcp import DHCP_SRV_NIC
from .testlib.dhcp import start_dhcp_server
from .testlib.dhcp import stop_dhcp_server
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml

DHCP_CLI_NIC = "dhcpcli-restore"
TEST_NET_NS = "nipart_dhcp_restore_test"
DEFAULT_TIMEOUT = 30


def _create_veth_pair(ifname, peer, peer_ns):
    exec_cmd(f"ip link add {ifname} type veth peer name {peer}".split())
    exec_cmd(f"ip link set {ifname} up".split())
    exec_cmd(f"ip link set {peer} netns {peer_ns}".split())
    exec_cmd(f"ip netns exec {peer_ns} ip link set {peer} up".split())
    exec_cmd(f"ip netns exec {peer_ns} ip link set lo up".split())


def _remove_veth_pair(ifname, peer_ns):
    exec_cmd(f"ip link del {ifname}".split(), check=False)
    exec_cmd(f"ip netns del {peer_ns}".split(), check=False)


def _has_dhcp_addr():
    rc, out, _ = exec_cmd(
        ["ip", "-4", "addr", "show", "dev", DHCP_CLI_NIC], check=False
    )
    return "192.0.2." in out and "dynamic" in out


def _has_gateway_route():
    rc, out, _ = exec_cmd(
        ["ip", "route", "show", "dev", DHCP_CLI_NIC], check=False
    )
    return any("default" in line for line in out.splitlines())


def _log_since(pos, text):
    if not os.path.exists(DAEMON_LOG):
        return False
    with open(DAEMON_LOG) as log_f:
        log_f.seek(pos)
        return text in log_f.read()


def test_dhcp_client_restored_after_daemon_restart():
    exec_cmd(f"ip netns del {TEST_NET_NS}".split(), check=False)
    exec_cmd(f"ip netns add {TEST_NET_NS}".split())
    _create_veth_pair(DHCP_CLI_NIC, DHCP_SRV_NIC, TEST_NET_NS)
    start_dhcp_server(TEST_NET_NS)

    try:
        nipart.apply(load_yaml(f"""---
                interfaces:
                - name: {DHCP_CLI_NIC}
                  type: ethernet
                  state: up
                  ipv4:
                    enabled: true
                    dhcp: true"""))
        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT, _has_dhcp_addr
        ), f"{DHCP_CLI_NIC} did not get a DHCPv4 lease"

        # Restart the daemon while the lease is still present in the
        # kernel.  The kernel state reports the address with `dhcp: true`,
        # so the boot apply sees no diff - the userspace DHCP client (it
        # died with the daemon) must be restored explicitly, otherwise the
        # lease expires without renewal.
        log_pos = 0
        if os.path.exists(DAEMON_LOG):
            log_pos = os.path.getsize(DAEMON_LOG)
        stop_daemon()
        start_daemon()

        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT,
            _log_since,
            log_pos,
            f"Restoring DHCPv4 client on interface {DHCP_CLI_NIC}",
        ), "DHCPv4 client not restored after daemon restart"
        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT, _log_since, log_pos, "got lease 192.0.2"
        ), "DHCPv4 client did not re-acquire the lease after daemon restart"
    finally:
        nipart.apply(load_yaml(f"""---
                interfaces:
                - name: {DHCP_CLI_NIC}
                  type: ethernet
                  state: absent"""))
        _remove_veth_pair(DHCP_CLI_NIC, TEST_NET_NS)
        stop_dhcp_server()


def test_dhcp_auto_gateway_false_after_daemon_restart():
    exec_cmd(f"ip netns del {TEST_NET_NS}".split(), check=False)
    exec_cmd(f"ip netns add {TEST_NET_NS}".split())
    _create_veth_pair(DHCP_CLI_NIC, DHCP_SRV_NIC, TEST_NET_NS)
    start_dhcp_server(TEST_NET_NS)

    try:
        nipart.apply(load_yaml(f"""---
                interfaces:
                - name: {DHCP_CLI_NIC}
                  type: ethernet
                  state: up
                  ipv4:
                    enabled: true
                    dhcp: true
                    auto-gateway: false"""))
        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT, _has_dhcp_addr
        ), f"{DHCP_CLI_NIC} did not get a DHCPv4 lease"
        assert not _has_gateway_route()

        # The kernel state carries the DHCP address with `dhcp: true` but
        # never the config-only `auto_gateway` property.  Whatever path
        # restarts the DHCP client after the daemon restart (the boot apply
        # sees a diff because of `auto-gateway`, or the client is restored
        # explicitly when the state matches), the client must keep honoring
        # `auto-gateway: false`, otherwise the gateway route would be added
        # on the first renewal.
        log_pos = 0
        if os.path.exists(DAEMON_LOG):
            log_pos = os.path.getsize(DAEMON_LOG)
        stop_daemon()
        start_daemon()

        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT, _log_since, log_pos, "got lease 192.0.2"
        ), "DHCPv4 client did not re-acquire the lease after daemon restart"
        assert not _has_gateway_route()
    finally:
        nipart.apply(load_yaml(f"""---
                interfaces:
                - name: {DHCP_CLI_NIC}
                  type: ethernet
                  state: absent"""))
        _remove_veth_pair(DHCP_CLI_NIC, TEST_NET_NS)
        stop_dhcp_server()
