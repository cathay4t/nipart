# SPDX-License-Identifier: Apache-2.0

import nipart

from .testlib.statelib import load_yaml, show_only, show_saved_only
from .testlib.veth import veth_interface

TEST_VETH = "veth-mem-only0"
TEST_VETH_PEER = "veth-mem-only1"
PERSIST_MTU = 1400
MEM_ONLY_MTU = 1300


def test_memory_only_apply_not_persisted():
    with veth_interface(TEST_VETH, TEST_VETH_PEER):
        nipart.apply(load_yaml(f"""---
            interfaces:
            - name: {TEST_VETH}
              type: ethernet
              mtu: {PERSIST_MTU}"""))

        assert show_only(TEST_VETH)["mtu"] == PERSIST_MTU
        assert show_saved_only(TEST_VETH)["mtu"] == PERSIST_MTU

        nipart.apply(
            load_yaml(f"""---
                interfaces:
                - name: {TEST_VETH}
                  type: ethernet
                  mtu: {MEM_ONLY_MTU}"""),
            memory_only=True,
        )

        # The change must be effective in running state
        assert show_only(TEST_VETH)["mtu"] == MEM_ONLY_MTU
        # but not persisted into saved state
        assert show_saved_only(TEST_VETH)["mtu"] == PERSIST_MTU
