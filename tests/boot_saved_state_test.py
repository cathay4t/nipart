# SPDX-License-Identifier: Apache-2.0

import os
import time

from .conftest import DAEMON_LOG, start_daemon, stop_daemon
from .testlib.cmdlib import exec_cmd
from .testlib.retry import retry_till_true_or_timeout

SAVED_STATE_FILE = "/etc/nipart/states/internal/applied.yml"
TEST_VETH = "veth-saved-mac0"
TEST_VETH_PEER = "veth-saved-mac1"
TEST_MAC = "00:23:45:67:89:1b"
TEST_IP = "192.0.2.99"
TEST_MTU = 1280
ROUTE_NEXTHOP = "192.0.2.1"
DEFAULT_TIMEOUT = 30

# A saved config whose NIC does not exist on this host: it must not block
# the boot apply and must be activated by the monitor worker when a NIC
# carrying the saved MAC appears.
SAVED_STATE = f"""---
version: 1
routes:
  config:
  - destination: 172.25.80.0/24
    next-hop-interface: wan0
    next-hop-address: {ROUTE_NEXTHOP}
    metric: 103
    table-id: 254
interfaces:
- name: wan0
  profile-name: wan0
  type: ethernet
  identifier: mac-address
  state: up
  mac-address: {TEST_MAC}
  mtu: {TEST_MTU}
  ipv4:
    enabled: true
    dhcp: false
    address:
    - ip: {TEST_IP}
      prefix-length: 24
"""


def _iface_has_ip_and_mtu(iface_name):
    rc, out, _ = exec_cmd(
        ["ip", "-4", "addr", "show", "dev", iface_name], check=False
    )
    return (
        rc == 0
        and TEST_IP in out
        and "state UP" in out
        and f"mtu {TEST_MTU}" in out
    )


def _iface_has_route(iface_name):
    rc, out, _ = exec_cmd(
        ["ip", "route", "show", "dev", iface_name], check=False
    )
    return "172.25.80.0/24" in out and ROUTE_NEXTHOP in out


def _route_exists():
    rc, out, _ = exec_cmd(
        ["ip", "route", "show", "172.25.80.0/24"], check=False
    )
    return rc == 0 and "172.25.80.0/24" in out


def _log_since(pos, text):
    if not os.path.exists(DAEMON_LOG):
        return False
    with open(DAEMON_LOG) as log_f:
        log_f.seek(pos)
        return text in log_f.read()


def test_saved_config_without_nic_not_blocking_boot_and_activated_on_hotplug():
    # Ensure the veth does not exist and the daemon is stopped.
    exec_cmd(["ip", "link", "del", TEST_VETH], check=False)
    stop_daemon()

    try:
        # Save a config whose NIC (identified by MAC) is not present.
        os.makedirs(os.path.dirname(SAVED_STATE_FILE), exist_ok=True)
        with open(SAVED_STATE_FILE, "w") as state_f:
            state_f.write(SAVED_STATE)

        log_pos = 0
        if os.path.exists(DAEMON_LOG):
            log_pos = os.path.getsize(DAEMON_LOG)

        start_daemon()

        # The boot apply must not keep retrying for the absent NIC: within
        # a few seconds (the boot grace period) the config is left for the
        # monitor worker, and the old "Failed to apply all saved state
        # within 30 retries" error must never appear.
        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT,
            _log_since,
            log_pos,
            "is left for monitor worker to activate",
        ), (
            "Boot apply did not leave the absent-NIC config for the "
            "monitor worker"
        )
        assert not _log_since(
            log_pos, "Failed to apply all saved state"
        ), "Boot apply should not error out on the absent-NIC config"

        # Stay past the boot grace period: the config must remain dormant
        # (no route, no further boot retries) until the NIC actually
        # appears - it is the monitor worker that activates it, not the
        # boot apply.
        time.sleep(3)
        assert (
            not _route_exists()
        ), "Route via the missing NIC should not exist before plug-in"

        # Now the NIC appears (well after the boot grace period): the
        # monitor worker emits the link event and the saved config (IP,
        # MTU and route) is applied.
        exec_cmd(
            f"ip link add {TEST_VETH} address {TEST_MAC}"
            f" type veth peer name {TEST_VETH_PEER}".split()
        )
        exec_cmd(f"ip link set {TEST_VETH_PEER} up".split())

        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT, _iface_has_ip_and_mtu, TEST_VETH
        ), f"{TEST_VETH} not up with IP {TEST_IP} after plug-in"
        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT, _iface_has_route, TEST_VETH
        ), f"Route via {TEST_VETH} not added after plug-in"
    finally:
        exec_cmd(["ip", "link", "del", TEST_VETH], check=False)
        # Remove the test saved state so later tests start clean.
        if os.path.exists(SAVED_STATE_FILE):
            os.remove(SAVED_STATE_FILE)
        stop_daemon()
        start_daemon()
