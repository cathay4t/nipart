# SPDX-License-Identifier: Apache-2.0

import pytest

import nipart

from .testlib.cmdlib import exec_cmd
from .testlib.dhcp import DHCP_SRV_IP4
from .testlib.dhcp import DHCP_SRV_IP4_PREFIX
from .testlib.env import has_kernel_module
from .testlib.env import npt_path
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml
from .testlib.wifi import TEST_NET_NS
from .testlib.wifi import TEST_WIFI_PSK
from .testlib.wifi import TEST_WIFI_SSID_HIDDEN
from .testlib.wifi import WIFI_TEST_NIC
from .testlib.wifi import create_sim_wifi_nics
from .testlib.wifi import destroy_sim_wifi_nics
from .testlib.wifi import start_hostapd_hidden


@pytest.fixture(scope="module")
def wifi_hidden_env():
    create_sim_wifi_nics()
    exec_cmd("killall wpa_supplicant".split(), check=False)
    start_hostapd_hidden(TEST_NET_NS)
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


def ping_peer():
    try:
        exec_cmd(f"ping {DHCP_SRV_IP4} -c 1 -w 5".split())
    except Exception:
        return False
    return True


def link_is_up():
    output = exec_cmd(
        f"ip -br link show {WIFI_TEST_NIC}".split(), check=False
    )[1]
    return "UP" in output


@pytest.mark.skipif(
    not has_kernel_module("mac80211_hwsim"),
    reason="Does not have 'mac80211_hwsim' kernel module",
)
class TestWifiHidden:
    def test_wifi_hidden_iface_static_ip(
        self, clean_up, wifi_hidden_env  # noqa: F811
    ):
        nipart.apply(load_yaml(f"""---
                interfaces:
                  - name: {WIFI_TEST_NIC}
                    type: wifi-phy
                    state: up
                    wifi:
                      ssid: {TEST_WIFI_SSID_HIDDEN}
                      password: {TEST_WIFI_PSK}
                      hidden: true
                    ipv4:
                      enabled: true
                      dhcp: false
                      address:
                        - ip: {DHCP_SRV_IP4_PREFIX}.99
                          prefix-length: 24"""))
        assert retry_till_true_or_timeout(10, ping_peer)

    def test_wifi_hidden_iface_dhcpv4(
        self, clean_up, wifi_hidden_env  # noqa: F811
    ):
        nipart.apply(load_yaml(f"""---
                interfaces:
                  - name: {WIFI_TEST_NIC}
                    type: wifi-phy
                    state: up
                    wifi:
                      ssid: {TEST_WIFI_SSID_HIDDEN}
                      password: {TEST_WIFI_PSK}
                      hidden: true
                    ipv4:
                      enabled: true
                      dhcp: true"""))
        assert retry_till_true_or_timeout(10, ping_peer)

    def test_wifi_scan_hides_hidden_ssid(
        self, clean_up, wifi_hidden_env  # noqa: F811
    ):
        nipart.apply(load_yaml(f"""---
                interfaces:
                  - name: {WIFI_TEST_NIC}
                    type: wifi-phy
                    state: up
                    wifi:
                      ssid: {TEST_WIFI_SSID_HIDDEN}
                      password: {TEST_WIFI_PSK}
                      hidden: true
                    ipv4:
                      enabled: true
                      dhcp: false
                      address:
                        - ip: {DHCP_SRV_IP4_PREFIX}.99
                          prefix-length: 24"""))
        assert retry_till_true_or_timeout(10, ping_peer)

        # Disconnect the shuli client before scanning: a standalone scan
        # can hit EBUSY while the client is connected.  The kernel BSS
        # cache keeps the hidden SSID from the connection.
        nipart.apply(
            load_yaml(f"""---
                interfaces:
                  - name: {WIFI_TEST_NIC}
                    type: wifi-phy
                    state: down"""),
            verify_change=False,
        )
        exec_cmd(f"ip link set {WIFI_TEST_NIC} up".split())
        retry_till_true_or_timeout(5, link_is_up)

        output = exec_cmd([npt_path(), "wifi", "scan"])[1]
        assert TEST_WIFI_SSID_HIDDEN not in output

        output = exec_cmd([npt_path(), "wifi", "scan", "--show-hidden"])[1]
        assert TEST_WIFI_SSID_HIDDEN in output


@pytest.mark.skipif(
    not has_kernel_module("mac80211_hwsim"),
    reason="Does not have 'mac80211_hwsim' kernel module",
)
class TestWifiHiddenAutoConnect:
    """Apply hidden SSID state before the AP comes up; verify the daemon
    auto-connects once the hidden AP becomes available."""

    @pytest.fixture(autouse=True)
    def setup_and_teardown(self):
        create_sim_wifi_nics()
        exec_cmd("killall wpa_supplicant".split(), check=False)
        yield
        destroy_sim_wifi_nics()

    def test_auto_connect_after_ap_starts(self):
        # Apply the hidden SSID config before hostapd is running.
        # The daemon saves the state and holds the connection attempt;
        # the AP is not yet broadcasting.
        try:
            nipart.apply(
                load_yaml(f"""---
                    interfaces:
                      - name: {WIFI_TEST_NIC}
                        type: wifi-phy
                        state: up
                        wifi:
                          ssid: {TEST_WIFI_SSID_HIDDEN}
                          password: {TEST_WIFI_PSK}
                          hidden: true
                        ipv4:
                          enabled: true
                          dhcp: false
                          address:
                            - ip: {DHCP_SRV_IP4_PREFIX}.99
                              prefix-length: 24"""),
                verify_change=False,
            )
        except Exception:
            # Expected: AP not up yet, daemon saves config for retry.
            pass

        # Now bring up the hidden AP.
        start_hostapd_hidden(TEST_NET_NS)

        # Nipart should auto-connect via directed probe (hidden_ssids).
        assert retry_till_true_or_timeout(
            30, ping_peer
        ), "hidden SSID did not auto-connect after AP came up"
