# SPDX-License-Identifier: Apache-2.0

from .testlib.statelib import show_only
from .testlib.statelib import state_match
from .testlib.statelib import load_yaml

import pytest

import nipart
from nipart import NipartStateKind


def test_query_loopback():
    iface_state = show_only("lo")

    assert iface_state["name"] == "lo"
    assert iface_state["mtu"] == 65536
    assert iface_state["mac-address"] == "00:00:00:00:00:00"
    assert state_match(
        {
            "address": [{"ip": "127.0.0.1", "prefix-length": 8}],
            "enabled": True,
        },
        iface_state["ipv4"],
    )
    assert state_match(
        {
            "address": [{"ip": "::1", "prefix-length": 128}],
            "enabled": True,
        },
        iface_state["ipv6"],
    )


@pytest.fixture
def clean_up_loopback():
    yield
    nipart.apply(load_yaml("""---
                version: 1
                interfaces:
                - name: lo
                  type: loopback
                  state: absent
                """))
    iface_state = show_only("lo")
    assert state_match(
        [
            {"ip": "127.0.0.1", "prefix-length": 8},
        ],
        iface_state["ipv4"]["address"],
    )
    assert state_match(
        [
            {"ip": "::1", "prefix-length": 128},
        ],
        iface_state["ipv6"]["address"],
    )


@pytest.mark.parametrize(
    "query_kind",
    [
        NipartStateKind.RUNNING,
        NipartStateKind.SAVED,
    ],
    ids=[
        "query_running",
        "query_saved",
    ],
)
