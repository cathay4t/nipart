# SPDX-License-Identifier: Apache-2.0

import os

import nipart
import pytest

from .conftest import DAEMON_LOG
from .testlib.cmdlib import exec_cmd
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml

TEST_VETH = "veth-appfail0"
TEST_VETH_PEER = "veth-appfail1"
TEST_PROFILE = "appfail-prof0"
TEST_MAC = "02:00:00:00:00:07"
DEFAULT_TIMEOUT = 30


def _has_delayed_event_since(pos, iface_name):
    if not os.path.exists(DAEMON_LOG):
        return False
    with open(DAEMON_LOG) as log_f:
        log_f.seek(pos)
        return f"Emit delayed event on {iface_name}" in log_f.read()


def _apply_unsupported_ecmp_route():
    """Apply a state which fails after the monitor was paused.

    The kernel backend rejects ECMP routes, so this desired state passes
    the schema sanitize stage but fails in `apply_routes()`, i.e. after
    the daemon has paused the interface monitor.
    """
    with pytest.raises(nipart.NipartError) as err:
        nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_VETH}
                type: veth
                identifier: mac-address
                mac-address: {TEST_MAC}
                state: up
                ipv4:
                  enabled: true
                  dhcp: false
                  address:
                    - ip: 192.0.2.7
                      prefix-length: 24
            routes:
              config:
                - destination: 198.51.100.0/24
                  next-hop-interface: {TEST_VETH}
                  metric: 100
                  weight: 2"""))
    assert err.value.kind == "no-support", (
        f"Expected no-support apply error, got {err.value.kind}: "
        f"{err.value.msg}"
    )


def test_monitor_alive_after_failed_apply():
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
        _apply_unsupported_ecmp_route()

        # The failed apply must not leave the monitor paused: a link
        # event occurring afterwards still has to be delivered to the
        # event worker.
        exec_cmd(f"ip link set {TEST_VETH_PEER} down".split())
        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT,
            _has_delayed_event_since,
            log_pos,
            TEST_VETH,
        ), (
            f"{TEST_VETH} link event was not emitted after a failed apply: "
            "the monitor worker was left paused"
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
