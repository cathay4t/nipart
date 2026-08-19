# SPDX-License-Identifier: Apache-2.0

import os
import time

import nipart
from .conftest import DAEMON_LOG
from .testlib.cmdlib import exec_cmd
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml

TEST_VETH = "veth-mon0"
TEST_VETH_PEER = "veth-mon1"
TEST_PROFILE = "mon-prof0"
TEST_MAC = "02:00:00:00:00:01"
DEFAULT_TIMEOUT = 30


def _log_count_since(pos, text):
    if not os.path.exists(DAEMON_LOG):
        return 0
    with open(DAEMON_LOG) as log_f:
        log_f.seek(pos)
        return log_f.read().count(text)


def _has_delayed_event_since(pos, iface_name):
    return _log_count_since(pos, f"Emit delayed event on {iface_name}") > 0


def test_link_down_does_not_loop_monitor_events():
    exec_cmd(["ip", "link", "del", TEST_VETH], check=False)
    try:
        exec_cmd(
            f"ip link add {TEST_VETH} address {TEST_MAC}"
            f" type veth peer name {TEST_VETH_PEER}".split()
        )
        exec_cmd(f"ip link set {TEST_VETH_PEER} up".split())
        nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_PROFILE}
                type: ethernet
                identifier: mac-address
                mac-address: {TEST_MAC}
                state: up"""))

        log_pos = os.path.getsize(DAEMON_LOG)
        # Keep the veth link-state down while the saved profile keeps
        # asking for `state: up`: this mirrors a wifi-phy that stays
        # link-down (no carrier/SSID) after apply.
        exec_cmd(f"ip link set {TEST_VETH_PEER} down".split())

        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT,
            _has_delayed_event_since,
            log_pos,
            TEST_VETH,
        ), (
            f"{TEST_VETH} link-down event was not emitted by the "
            "monitor worker"
        )
        first_count = _log_count_since(
            log_pos, f"Emit delayed event on {TEST_VETH}"
        )
        # A monitor bug re-opened the netlink socket on every event apply,
        # causing the same link-down event to be re-queued every 10 seconds.
        # Wait longer than one full cycle: the count must not grow.
        time.sleep(30)
        assert (
            _log_count_since(log_pos, f"Emit delayed event on {TEST_VETH}")
            == first_count
        ), (
            f"{TEST_VETH} link-down event kept being re-emitted by the "
            "monitor worker"
        )
    finally:
        try:
            nipart.apply(load_yaml(f"""---
                interfaces:
                  - name: {TEST_PROFILE}
                    type: ethernet
                    identifier: mac-address
                    mac-address: {TEST_MAC}
                    state: absent"""))
        finally:
            exec_cmd(["ip", "link", "del", TEST_VETH], check=False)
