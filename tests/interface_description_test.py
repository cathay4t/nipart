# SPDX-License-Identifier: Apache-2.0

import nipart

from .testlib.statelib import load_yaml, show_only, show_saved_only
from .testlib.veth import veth_interface

TEST_VETH = "veth-desc0"
TEST_VETH_PEER = "veth-desc1"
TEST_DESCRIPTION = "Main interface connected to switch S1"


def test_interface_description_persisted_and_in_running_state():
    with veth_interface(TEST_VETH, TEST_VETH_PEER):
        nipart.apply(load_yaml(f"""---
                interfaces:
                - name: {TEST_VETH}
                  type: ethernet
                  state: up
                  description: "{TEST_DESCRIPTION}"
                """))

        saved_iface = show_saved_only(TEST_VETH)
        assert saved_iface is not None
        assert saved_iface.get("description") == TEST_DESCRIPTION

        running_iface = show_only(TEST_VETH)
        assert running_iface is not None
        assert running_iface.get("description") == TEST_DESCRIPTION


def test_interface_description_can_be_cleared():
    with veth_interface(TEST_VETH, TEST_VETH_PEER):
        nipart.apply(load_yaml(f"""---
                interfaces:
                - name: {TEST_VETH}
                  type: ethernet
                  state: up
                  description: "{TEST_DESCRIPTION}"
                """))

        nipart.apply(load_yaml(f"""---
                interfaces:
                - name: {TEST_VETH}
                  type: ethernet
                  state: up
                  description: ""
                """))

        saved_iface = show_saved_only(TEST_VETH)
        assert saved_iface is not None
        assert saved_iface.get("description") == ""

        running_iface = show_only(TEST_VETH)
        assert running_iface is not None
        assert running_iface.get("description") == ""
