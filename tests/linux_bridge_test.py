# SPDX-License-Identifier: Apache-2.0

import pytest

import nipart

from .testlib.statelib import load_yaml
from .testlib.statelib import show_only
from .testlib.statelib import state_match

TEST_PORT1 = "dummy1"
TEST_PORT2 = "dummy2"
TEST_BRIDGE_NIC = "br0"


@pytest.fixture
def linux_bridge_over_dummy():
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_BRIDGE_NIC}
                type: linux-bridge
                state: up
                bridge:
                  vlan:
                    mode: access
                    tag: 300
                  ports:
                    - name: {TEST_PORT2}
                      stp-hairpin-mode: true
                      stp-priority: 20
                      stp-path-cost: 200
                      vlan:
                        mode: trunk
                        trunk-tags:
                        - id: 400
                        - id-range:
                            min: 500
                            max: 600
                    - name: {TEST_PORT1}
                      stp-hairpin-mode: true
                      stp-priority: 10
                      stp-path-cost: 100
                      vlan:
                        mode: access
                        tag: 700
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
              - name: {TEST_BRIDGE_NIC}
                type: linux-bridge
                state: absent
              - name: {TEST_PORT1}
                type: dummy
                state: absent
              - name: {TEST_PORT2}
                type: dummy
                state: absent
            """))


def test_create_and_remove_linux_bridge(linux_bridge_over_dummy):
    linux_bridge_iface = show_only(TEST_BRIDGE_NIC)
    assert state_match(
        [
            {
                "name": TEST_PORT1,
                "stp-priority": 10,
                "stp-path-cost": 100,
                "stp-hairpin-mode": True,
            },
            {
                "name": TEST_PORT2,
                "stp-priority": 20,
                "stp-path-cost": 200,
                "stp-hairpin-mode": True,
            },
        ],
        linux_bridge_iface["bridge"]["ports"],
    )


@pytest.mark.parametrize(
    "opt_key_value",
    [
        ("group-addr", "01:80:C2:00:00:05"),
        ("group-fwd-mask", 248),
        ("hash-max", 256),
        ("mac-ageing-time", 1000),
        ("multicast-last-member-count", 10),
        ("multicast-last-member-interval", 200),
        ("multicast-membership-interval", 30000),
        ("multicast-querier", True),
        ("multicast-querier-interval", 26500),
        ("multicast-query-interval", 12800),
        ("multicast-query-response-interval", 2000),
        ("multicast-query-use-ifaddr", True),
        ("multicast-router", "disabled"),
        ("multicast-snooping", True),
        ("multicast-startup-query-count", 4),
        ("multicast-startup-query-interval", 4000),
        ("vlan-protocol", "802.1ad"),
    ],
    ids=[
        "group-addr",
        "group-fwd-mask",
        "hash-max",
        "mac-ageing-time",
        "multicast-last-member-count",
        "multicast-last-member-interval",
        "multicast-membership-interval",
        "multicast-querier",
        "multicast-querier-interval",
        "multicast-query-interval",
        "multicast-query-response-interval",
        "multicast-query-use-ifaddr",
        "multicast-router",
        "multicast-snooping",
        "multicast-startup-query-count",
        "multicast-startup-query-interval",
        "vlan-protocol",
    ],
)
