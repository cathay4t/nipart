# SPDX-License-Identifier: Apache-2.0

"""WIFI config applied before the AP exists: nipart must keep waiting in
the background (shuli scheduled scan / host scan backoff) and report the
connection once hostapd starts later.

mac80211_hwsim does not implement sched_scan_start, so shuli falls back
to host-side exponential backoff here; that fallback path is what this
test exercises end-to-end.
"""

import os

import pytest

import nipart

from .testlib.cmdlib import exec_cmd
from .testlib.env import has_kernel_module
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml
from .testlib.wifi import TEST_WIFI_PSK
from .testlib.wifi import TEST_WIFI_SSID
from .testlib.wifi import WIFI_TEST_NIC
from .testlib.wifi import create_sim_wifi_nics
from .testlib.wifi import destroy_sim_wifi_nics
from .testlib.wifi import start_hostapd

pytestmark = pytest.mark.skipif(
    os.geteuid() != 0,
    reason="root required (mac80211_hwsim, netns and hostapd)",
)

# The AP appears only after apply: with host-scan backoff on
# mac80211_hwsim (10 -> 20 -> ... -> 300 seconds between scans), the
# connect may take several backoff cycles, so keep the deadline in line
# with the plugin's bounded hunt.
CONNECT_TIMEOUT = 600


@pytest.fixture(scope="module")
def wifi_env_ap_later():
    # mac80211_hwsim + netns with both radios present but hostapd NOT
    # running yet: the AP is started inside the test, after the apply.
    create_sim_wifi_nics()
    # nipart.show() may have (re)started wpa_supplicant holding the
    # hwsim NIC; kill it so shuli owns the nl80211 connection.
    exec_cmd("killall wpa_supplicant".split(), check=False)
    yield
    destroy_sim_wifi_nics()


@pytest.fixture
def clean_up():
    yield
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {WIFI_TEST_NIC}
                type: wifi-phy
                state: absent"""))


def get_wifi_ssid():
    state = nipart.show()
    for iface in state["interfaces"]:
        if iface.get("name") == WIFI_TEST_NIC:
            return (iface.get("wifi") or {}).get("ssid")
    return None


def is_wifi_connected():
    return get_wifi_ssid() == TEST_WIFI_SSID


@pytest.mark.skipif(
    not has_kernel_module("mac80211_hwsim"),
    reason=("Does not have 'mac80211_hwsim' kernel module "),
)
class TestWifiApStartsLater:
    def test_wifi_connects_after_ap_starts_later(  # noqa: F811
        self, clean_up, wifi_env_ap_later
    ):
        # Apply the WIFI config while no AP is present.  Verification
        # would fail immediately (the SSID cannot be connected yet), so
        # skip it: the plugin keeps hunting in the background and the
        # connection completes once the AP shows up.
        nipart.apply(
            load_yaml(f"""---
                interfaces:
                  - name: {WIFI_TEST_NIC}
                    type: wifi-phy
                    state: up
                    wifi:
                      ssid: {TEST_WIFI_SSID}
                      password: {TEST_WIFI_PSK}"""),
            verify_change=False,
        )

        # Sanity: with no AP running, nipart must not report the SSID
        # yet.
        assert not is_wifi_connected()

        # Start the AP now: nipart should pick it up on a later
        # background scan and connect on its own.  A longer timeout is
        # needed because `iw scan` on the test NIC returns -EBUSY while
        # shuli's own scan is in flight.
        start_hostapd(timeout=60)
        assert retry_till_true_or_timeout(
            CONNECT_TIMEOUT, is_wifi_connected
        ), (
            f"nipart did not connect to {TEST_WIFI_SSID} after the AP "
            f"appeared; show() = {nipart.show()}"
        )
