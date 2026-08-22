# SPDX-License-Identifier: Apache-2.0

import re

import pytest

import nipart

from .conftest import CLI_PATH
from .testlib.cmdlib import exec_cmd
from .testlib.dhcp import DHCP_SRV_IP4
from .testlib.dhcp import DHCP_SRV_IP4_PREFIX
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


def ping_peer():
    try:
        exec_cmd(f"ping {DHCP_SRV_IP4} -c 1 -w 5".split())
    except Exception:
        return False
    return True


@pytest.mark.skipif(
    not has_kernel_module("mac80211_hwsim"),
    reason=("Does not have 'mac80211_hwsim' module "),
)
class TestWifi:
    def connected_ssid(self):
        output = exec_cmd(f"iw dev {WIFI_TEST_NIC} link".split(), check=False)[
            1
        ]
        match = re.search(r"SSID: (.+)", output)
        return match.group(1) if match else None

    def test_wifi_iface_static_ip(self, clean_up, wifi_env):  # noqa: F811
        nipart.apply(load_yaml(f"""---
                interfaces:
                  - name: {WIFI_TEST_NIC}
                    type: wifi-phy
                    state: up
                    wifi:
                      ssid: {TEST_WIFI_SSID}
                      password: {TEST_WIFI_PSK}
                    ipv4:
                      enabled: true
                      dhcp: false
                      address:
                        - ip: {DHCP_SRV_IP4_PREFIX}.99
                          prefix-length: 24"""))
        assert retry_till_true_or_timeout(5, ping_peer)

    def test_wifi_iface_dhcpv4(self, clean_up, wifi_env):  # noqa: F811
        nipart.apply(load_yaml(f"""---
                interfaces:
                  - name: {WIFI_TEST_NIC}
                    type: wifi-phy
                    state: up
                    wifi:
                      ssid: {TEST_WIFI_SSID}
                      password: {TEST_WIFI_PSK}
                    ipv4:
                      enabled: true
                      dhcp: true"""))
        assert retry_till_true_or_timeout(5, ping_peer)

    def test_wifi_off_scan_fails_and_up_restores(
        self, clean_up, wifi_env  # noqa: F811
    ):
        nipart.apply(load_yaml(f"""---
                interfaces:
                  - name: {WIFI_TEST_NIC}
                    type: wifi-phy
                    state: up
                    wifi:
                      ssid: {TEST_WIFI_SSID}
                      password: {TEST_WIFI_PSK}
                    ipv4:
                      enabled: true
                      dhcp: false
                      address:
                        - ip: {DHCP_SRV_IP4_PREFIX}.99
                          prefix-length: 24"""))
        assert retry_till_true_or_timeout(5, ping_peer)
        assert self.connected_ssid() == TEST_WIFI_SSID
        try:
            rc, out, err = exec_cmd([CLI_PATH, "wifi", "off"], check=False)
            assert rc == 0, f"npt wifi off failed:\n{out}\n{err}"
            assert "WIFI is off" in out, out
            assert retry_till_true_or_timeout(
                5, lambda: self.connected_ssid() is None
            )

            rc, out, err = exec_cmd([CLI_PATH, "wifi", "scan"], check=False)
            assert rc != 0, "npt wifi scan should fail while WIFI is off"
            assert "WIFI is off" in err, err

            rc, out, err = exec_cmd([CLI_PATH, "wifi", "on"], check=False)
            assert rc == 0, f"npt wifi on failed:\n{out}\n{err}"
            assert "WIFI is on" in out, out

            assert retry_till_true_or_timeout(5, ping_peer)
            assert self.connected_ssid() == TEST_WIFI_SSID
        finally:
            exec_cmd([CLI_PATH, "wifi", "on"], check=False)
