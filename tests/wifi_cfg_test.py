# SPDX-License-Identifier: Apache-2.0

import re

import nipart
import pytest

from .testlib.cmdlib import exec_cmd
from .testlib.env import has_kernel_module
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml
from .testlib.wifi import TEST_WIFI_PSK
from .testlib.wifi import TEST_WIFI_SSID
from .testlib.wifi import WIFI_TEST_NIC
from .testlib.wifi import wifi_env  # noqa: F401


@pytest.fixture
def clean_up():
    yield
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {WIFI_TEST_NIC}
                type: wifi-phy
                state: absent"""))


def connected_ssid():
    output = exec_cmd(f"iw dev {WIFI_TEST_NIC} link".split(), check=False)[1]
    match = re.search(r"SSID: (.+)", output)
    return match.group(1) if match else None


def wifi_cfg_yaml(state):
    return load_yaml(f"""---
    interfaces:
      - name: {TEST_WIFI_SSID}
        type: wifi-cfg
        state: {state}
        wifi:
          ssid: {TEST_WIFI_SSID}
          password: {TEST_WIFI_PSK}
          base-iface: {WIFI_TEST_NIC}""")


@pytest.mark.skipif(
    not has_kernel_module("mac80211_hwsim"),
    reason="Does not have 'mac80211_hwsim' module",
)
class TestWifiCfg:
    def test_unrelated_apply_keeps_connection(
        self, clean_up, wifi_env  # noqa: F811
    ):
        nipart.apply(wifi_cfg_yaml("up"))
        assert retry_till_true_or_timeout(
            30, lambda: connected_ssid() == TEST_WIFI_SSID
        )
        nipart.apply(load_yaml("""---
        interfaces:
          - name: lo
            type: loopback
            state: up"""))
        assert retry_till_true_or_timeout(
            10, lambda: connected_ssid() == TEST_WIFI_SSID
        )

    def test_wifi_cfg_down_and_absent_disconnects(
        self, clean_up, wifi_env  # noqa: F811
    ):
        nipart.apply(wifi_cfg_yaml("up"))
        assert retry_till_true_or_timeout(
            30, lambda: connected_ssid() == TEST_WIFI_SSID
        )
        nipart.apply(wifi_cfg_yaml("down"))
        assert retry_till_true_or_timeout(10, lambda: connected_ssid() is None)
        nipart.apply(wifi_cfg_yaml("absent"))
        assert retry_till_true_or_timeout(10, lambda: connected_ssid() is None)
