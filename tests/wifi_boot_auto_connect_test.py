# SPDX-License-Identifier: Apache-2.0

import re

import nipart
import pytest

from .conftest import restart_daemon  # noqa: F401
from .conftest import start_daemon
from .conftest import stop_daemon
from .testlib.cmdlib import exec_cmd
from .testlib.env import has_kernel_module
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml
from .testlib.wifi import TEST_NET_NS
from .testlib.wifi import TEST_WIFI_SSID_OPEN
from .testlib.wifi import WIFI_TEST_NIC
from .testlib.wifi import create_sim_wifi_nics
from .testlib.wifi import destroy_sim_wifi_nics
from .testlib.wifi import start_hostapd_open


@pytest.fixture(scope="module")
def wifi_open_env():
    create_sim_wifi_nics()
    exec_cmd("killall wpa_supplicant".split(), check=False)
    start_hostapd_open(TEST_NET_NS)
    yield
    destroy_sim_wifi_nics()


def connected_ssid():
    output = exec_cmd(
        f"iw dev {WIFI_TEST_NIC} link".split(), check=False
    )[1]
    match = re.search(r"SSID: (.+)", output)
    return match.group(1) if match else None


@pytest.mark.skipif(
    not has_kernel_module("mac80211_hwsim"),
    reason="Does not have 'mac80211_hwsim' kernel module",
)
class TestWifiBootAutoConnect:
    def test_open_wifi_cfg_reconnects_after_daemon_restart(
        self, wifi_open_env, restart_daemon  # noqa: F811
    ):
        try:
            nipart.apply(load_yaml(f"""---
                    interfaces:
                      - name: {WIFI_TEST_NIC}
                        type: wifi-phy
                        state: up
                      - name: {TEST_WIFI_SSID_OPEN}
                        type: wifi-cfg
                        state: up
                        wifi:
                          ssid: {TEST_WIFI_SSID_OPEN}"""))
            assert retry_till_true_or_timeout(
                30, lambda: connected_ssid() == TEST_WIFI_SSID_OPEN
            )

            # The daemon restart must re-apply the saved open wifi-cfg
            # profile: the wifi plugin is a fresh process and has no live
            # connection until the boot apply configures it.
            stop_daemon()
            # A daemon restart alone keeps the kernel association; drop it
            # so the boot apply has to reconnect, like after a real reboot.
            exec_cmd(f"ip link set {WIFI_TEST_NIC} down".split())
            assert retry_till_true_or_timeout(
                10, lambda: connected_ssid() is None
            )
            start_daemon()
            assert retry_till_true_or_timeout(
                60, lambda: connected_ssid() == TEST_WIFI_SSID_OPEN
            ), (
                "open wifi-cfg did not auto-connect after daemon restart: "
                f"{connected_ssid()}"
            )
        finally:
            # Drop the saved profiles so a later module starts clean.
            nipart.apply(load_yaml(f"""---
                    interfaces:
                      - name: {WIFI_TEST_NIC}
                        type: wifi-phy
                        state: absent
                      - name: {TEST_WIFI_SSID_OPEN}
                        type: wifi-cfg
                        state: absent"""))
