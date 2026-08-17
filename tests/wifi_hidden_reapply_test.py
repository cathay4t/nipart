# SPDX-License-Identifier: Apache-2.0

import re

import nipart
import pytest

from .testlib.cmdlib import exec_cmd
from .testlib.env import has_kernel_module
from .testlib.env import npt_path
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml
from .testlib.wifi import TEST_WIFI_PSK
from .testlib.wifi import TEST_WIFI_SSID
from .testlib.wifi import WIFI_TEST_NIC
from .testlib.wifi import wifi_env  # noqa: F401


@pytest.fixture
def clean_up():
    yield
    nipart.apply(
        load_yaml(
            f"""---
            interfaces:
              - name: {WIFI_TEST_NIC}
                type: wifi-phy
                state: absent"""
        )
    )


def connected_ssid():
    output = exec_cmd(f"iw dev {WIFI_TEST_NIC} link".split(), check=False)[1]
    match = re.search(r"SSID: (.+)", output)
    return match.group(1) if match else None


@pytest.mark.skipif(
    not has_kernel_module("mac80211_hwsim"),
    reason="Does not have 'mac80211_hwsim' module",
)
class TestWifiHiddenReapply:
    def test_apply_show_state_keeps_hidden_password(
        self, clean_up, wifi_env  # noqa: F811
    ):
        nipart.apply(
            load_yaml(
                f"""---
                interfaces:
                  - name: {TEST_WIFI_SSID}
                    type: wifi-cfg
                    state: up
                    wifi:
                      ssid: {TEST_WIFI_SSID}
                      password: {TEST_WIFI_PSK}
                      base-iface: {WIFI_TEST_NIC}"""
            )
        )
        assert retry_till_true_or_timeout(
            30, lambda: connected_ssid() == TEST_WIFI_SSID
        )

        output = exec_cmd(
            f"{npt_path()} show --saved {TEST_WIFI_SSID}".split()
        )[1]
        shown_state = load_yaml(output)
        shown_wifi = shown_state["interfaces"][0]["wifi"]
        assert shown_wifi["password"] == "<_hidden_>"

        nipart.apply(shown_state)

        assert retry_till_true_or_timeout(
            10, lambda: connected_ssid() == TEST_WIFI_SSID
        )
        client = nipart.NipartClient()
        state = client.query_network_state(
            nipart.NipartQueryOption(saved=True, include_secrets=True)
        )
        wifi = next(
            i for i in state["interfaces"] if i["name"] == TEST_WIFI_SSID
        )
        assert wifi["wifi"]["password"] == TEST_WIFI_PSK
