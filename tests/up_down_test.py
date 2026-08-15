# SPDX-License-Identifier: Apache-2.0

import nipart

from .conftest import CLI_PATH
from .testlib.cmdlib import exec_cmd
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml
from .testlib.statelib import show_only

TEST_DUMMY = "dummy-up-down0"


def _dummy_up():
    iface_state = show_only(TEST_DUMMY)
    return iface_state is not None and iface_state.get("state") == "up"


def _dummy_gone():
    return show_only(TEST_DUMMY) is None


def test_npt_up_down_virtual_iface():
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_DUMMY}
                type: dummy
                state: up
            """))
    try:
        assert retry_till_true_or_timeout(10, _dummy_up)

        rc, out, err = exec_cmd([CLI_PATH, "down", TEST_DUMMY], check=False)
        assert rc == 0, f"npt down failed:\n{out}\n{err}"
        assert retry_till_true_or_timeout(
            10, _dummy_gone
        ), f"{TEST_DUMMY} should have been removed from the kernel"

        rc, out, err = exec_cmd([CLI_PATH, "up", TEST_DUMMY], check=False)
        assert rc == 0, f"npt up failed:\n{out}\n{err}"
        assert retry_till_true_or_timeout(
            10, _dummy_up
        ), f"{TEST_DUMMY} should have been recreated from saved state"
    finally:
        nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_DUMMY}
                type: dummy
                state: absent
            """))


def test_npt_up_missing_profile_fails():
    rc, out, err = exec_cmd([CLI_PATH, "up", "no-such-profile"], check=False)
    assert rc != 0, "npt up should fail for a missing profile"
    assert "No interface or profile" in err, err
