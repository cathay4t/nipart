# SPDX-License-Identifier: Apache-2.0

import re

import nipart
import pytest

from .conftest import restart_daemon  # noqa: F401
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


def connected_ssid():
    output = exec_cmd(f"iw dev {WIFI_TEST_NIC} link".split(), check=False)[1]
    match = re.search(r"SSID: (.+)", output)
    return match.group(1) if match else None


@pytest.mark.skipif(
    not has_kernel_module("mac80211_hwsim"),
    reason="Does not have 'mac80211_hwsim' kernel module",
)
class TestWifiPhyLater:
    def test_wifi_cfg_connects_when_phy_appears_after_daemon_start(
        self, restart_daemon  # noqa: F811
    ):
        try:
            # Start with no wifi-phy at all: the daemon only has the saved
            # wifi-cfg profile.  The wifi-phy appears after the boot grace
            # period, so only the monitor worker can notice it and hand the
            # saved profile to the wifi plugin.
            exec_cmd("modprobe -r mac80211_hwsim".split(), check=False)
            exec_cmd(f"ip netns del {TEST_NET_NS}".split(), check=False)

            nipart.apply(load_yaml(f"""---
                    interfaces:
                      - name: {TEST_WIFI_SSID_OPEN}
                        type: wifi-cfg
                        state: up
                        wifi:
                          ssid: {TEST_WIFI_SSID_OPEN}"""))

            client = nipart.NipartClient()
            saved_state = client.query_network_state(
                nipart.NipartQueryOption.saved()
            )
            assert not any(
                iface.get("type") == "wifi-phy"
                for iface in saved_state["interfaces"]
            ), "test setup should not persist a wifi-phy profile"

            create_sim_wifi_nics()
            exec_cmd("killall wpa_supplicant".split(), check=False)
            start_hostapd_open(TEST_NET_NS)

            assert retry_till_true_or_timeout(
                60, lambda: connected_ssid() == TEST_WIFI_SSID_OPEN
            ), (
                "wifi-cfg did not connect after wifi-phy appeared later: "
                f"{connected_ssid()}"
            )
        finally:
            destroy_sim_wifi_nics()
            nipart.apply(load_yaml(f"""---
                    interfaces:
                      - name: {TEST_WIFI_SSID_OPEN}
                        type: wifi-cfg
                        state: absent"""))
