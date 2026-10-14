# SPDX-License-Identifier: Apache-2.0

import os
import subprocess
import time

import nipart
import pytest

from .conftest import CLI_PATH
from .conftest import DAEMON_LOG
from .conftest import start_daemon
from .conftest import stop_daemon
from .testlib.cmdlib import exec_cmd
from .testlib.env import has_kernel_module
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml
from .testlib.statelib import show_only
from .wifi_test import TEST_WIFI_SSID_HIDDEN
from .wifi_test import wifi_hidden_env  # noqa: F401

TEST_DUMMY = "dummy-up-down0"
RUNNING_DUMMY = "dummy-running0"
DOWN_VETH = "npt-down-veth0"
DOWN_VETH_PEER = "npt-down-veth1"
DOWN_VETH_IP = "198.51.100.99"
DOWN_VETH_GW = "198.51.100.1"
DOWN_VETH_MAC = "02:00:00:00:00:0a"


def _dummy_up():
    iface_state = show_only(TEST_DUMMY)
    return iface_state is not None and iface_state.get("state") == "up"


def _dummy_gone():
    return show_only(TEST_DUMMY) is None


def _down_veth_stays_down():
    iface_state = show_only(DOWN_VETH)
    if iface_state is None or iface_state.get("state") != "down":
        return False
    rc, out, _ = exec_cmd(
        ["ip", "-4", "route", "show", "dev", DOWN_VETH], check=False
    )
    return rc == 0 and "default via" not in out


def _down_veth_is_up_with_route():
    iface_state = show_only(DOWN_VETH)
    if iface_state is None or iface_state.get("state") != "up":
        return False
    rc, out, _ = exec_cmd(
        ["ip", "-4", "route", "show", "dev", DOWN_VETH], check=False
    )
    return rc == 0 and DOWN_VETH_GW in out


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


def test_npt_down_nonvirtual_iface_removes_routes():
    for iface in (DOWN_VETH, DOWN_VETH_PEER):
        exec_cmd(["ip", "link", "del", iface], check=False)
    exec_cmd(
        [
            "ip",
            "link",
            "add",
            DOWN_VETH,
            "address",
            DOWN_VETH_MAC,
            "type",
            "veth",
            "peer",
            "name",
            DOWN_VETH_PEER,
        ],
        check=True,
    )
    exec_cmd(["ip", "link", "set", DOWN_VETH_PEER, "up"], check=True)
    try:
        nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {DOWN_VETH}
                type: ethernet
                state: up
                identifier: mac-address
                mac-address: {DOWN_VETH_MAC}
                ipv4:
                  enabled: true
                  dhcp: false
                  address:
                    - ip: {DOWN_VETH_IP}
                      prefix-length: 24
            routes:
              config:
                - destination: 0.0.0.0/0
                  next-hop-interface: {DOWN_VETH}
                  next-hop-address: {DOWN_VETH_GW}
                  metric: 100
                  table-id: 254
            """))
        assert retry_till_true_or_timeout(
            10, _down_veth_is_up_with_route
        ), f"{DOWN_VETH} should be up with default route before `npt down`"

        rc, out, err = exec_cmd([CLI_PATH, "down", DOWN_VETH], check=False)
        assert rc == 0, f"npt down failed:\n{out}\n{err}"
        # The route must stay gone; before the explicit-down tracking was
        # added, the monitor link dump re-applied the saved config and
        # brought the route back within a second.
        time.sleep(2)
        assert _down_veth_stays_down(), (
            f"{DOWN_VETH} should stay down without its default route after "
            "`npt down`"
        )

        rc, out, err = exec_cmd([CLI_PATH, "up", DOWN_VETH], check=False)
        assert rc == 0, f"npt up failed:\n{out}\n{err}"
        assert retry_till_true_or_timeout(10, _down_veth_is_up_with_route), (
            f"{DOWN_VETH} should be restored with its default route by "
            "`npt up`"
        )
    finally:
        exec_cmd(["ip", "link", "del", DOWN_VETH], check=False)
        nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {DOWN_VETH}
                type: ethernet
                state: absent
            """))


def test_npt_up_missing_profile_fails():
    rc, out, err = exec_cmd([CLI_PATH, "up", "no-such-profile"], check=False)
    assert rc != 0, "npt up should fail for a missing profile"
    assert "No interface or profile" in err, err


def _daemon_log_since(pos, mark, timeout=20):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with open(DAEMON_LOG, encoding="utf-8", errors="replace") as fd:
                fd.seek(pos)
                if mark in fd.read():
                    return True
        except FileNotFoundError:
            pass
        time.sleep(0.2)
    return False


@pytest.mark.skipif(
    not has_kernel_module("mac80211_hwsim"),
    reason="Does not have 'mac80211_hwsim' module",
)
def test_npt_up_missing_profile_fails_while_daemon_busy(  # noqa: F811
    wifi_hidden_env,  # noqa: F811
):
    """A missing profile must fail even while a daemon transaction is
    waiting: the on-going apply keeps the transaction lock, but looking
    up the target does not need it, so the command must not hang.
    """
    connect_proc = None
    try:
        # A `npt wifi connect` to a hidden SSID without `--hidden` is
        # never found by shuli, so the daemon keeps the transaction busy
        # for the whole WIFI connection wait.
        log_pos = os.path.getsize(DAEMON_LOG)
        connect_proc = subprocess.Popen(
            [
                CLI_PATH,
                "wifi",
                "connect",
                TEST_WIFI_SSID_HIDDEN,
                "--no-pass",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        assert _daemon_log_since(
            log_pos, "Waiting for WIFI connection"
        ), "daemon never started the WIFI connection wait"

        proc = subprocess.run(
            [CLI_PATH, "up", "no-such-profile"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=10,
        )
        err = proc.stderr.decode("utf-8")
        assert proc.returncode != 0, "npt up should fail for a missing profile"
        assert "No interface or profile" in err, err
    finally:
        if connect_proc is not None:
            connect_proc.kill()
            connect_proc.wait()
        # The abandoned connect keeps the old daemon busy for up to two
        # WIFI connect timeouts; restart it so the rest of the session
        # is not blocked.
        stop_daemon()
        start_daemon()


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
