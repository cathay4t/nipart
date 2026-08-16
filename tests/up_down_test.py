# SPDX-License-Identifier: Apache-2.0

import nipart

from .conftest import CLI_PATH
from .testlib.cmdlib import exec_cmd
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml
from .testlib.statelib import show_only

TEST_DUMMY = "dummy-up-down0"
RUNNING_DUMMY = "dummy-running0"


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
                ipv4:
                  enabled: true
                  dhcp: false
                  address:
                    - ip: 198.51.100.1
                      prefix-length: 24
            routes:
              config:
                - destination: 198.51.100.0/24
                  next-hop-interface: {TEST_DUMMY}
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


def test_npt_default_shows_configured_ifaces():
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_DUMMY}
                type: dummy
                state: up
            """))
    try:
        assert retry_till_true_or_timeout(10, _dummy_up)
        rc, out, err = exec_cmd([CLI_PATH], check=False)
        assert rc == 0, f"npt without arguments failed:\n{out}\n{err}"
        assert f"{TEST_DUMMY}: state" in out, out
        assert "link dummy" in out, out
        for cmd in (
            [CLI_PATH, "brief", TEST_DUMMY],
            [CLI_PATH, "b", TEST_DUMMY],
        ):
            rc, out, err = exec_cmd(cmd, check=False)
            assert rc == 0, f"npt brief failed:\n{out}\n{err}"
            assert f"{TEST_DUMMY}: state" in out, out
            assert "link dummy" in out, out
        rc, out, err = exec_cmd(
            [CLI_PATH, "brief", "no-such-profile"], check=False
        )
        assert rc != 0, "npt brief should fail for a missing profile"
        assert "not found" in err, err
    finally:
        nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_DUMMY}
                type: dummy
                state: absent
            """))


def test_npt_brief_running_shows_unconfigured_iface():
    exec_cmd(["ip", "link", "del", RUNNING_DUMMY], check=False)
    exec_cmd(["ip", "link", "add", RUNNING_DUMMY, "type", "dummy"], check=True)
    try:
        rc, out, err = exec_cmd([CLI_PATH, "brief"], check=False)
        assert rc == 0, f"npt brief failed:\n{out}\n{err}"
        assert RUNNING_DUMMY not in out, out

        for cmd in (
            [CLI_PATH, "brief", "--running"],
            [CLI_PATH, "brief", "-r"],
            [CLI_PATH, "b", "-r"],
            [CLI_PATH, "brief", "--running", RUNNING_DUMMY],
            [CLI_PATH, "b", "-r", RUNNING_DUMMY],
        ):
            rc, out, err = exec_cmd(cmd, check=False)
            assert rc == 0, f"npt brief --running failed:\n{out}\n{err}"
            assert f"{RUNNING_DUMMY}: state" in out, out
            assert "link dummy" in out, out
    finally:
        exec_cmd(["ip", "link", "del", RUNNING_DUMMY], check=False)
