# SPDX-License-Identifier: Apache-2.0

import re

import nipart
import pytest

from .testlib.cmdlib import exec_cmd
from .testlib.dhcp import DHCP_SRV_IP4
from .testlib.dhcp import DHCP_SRV_IP4_PREFIX
from .testlib.dhcp import DNSMASQ_PID_PATH
from .testlib.dhcp import stop_dhcp_server
from .testlib.env import has_kernel_module
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml
from .testlib.statelib import show_only
from .testlib.wifi import DHCP_SRV_NIC
from .testlib.wifi import TEST_NET_NS
from .testlib.wifi import TEST_WIFI_PSK
from .testlib.wifi import TEST_WIFI_SSID
from .testlib.wifi import WIFI_TEST_NIC
from .testlib.wifi import wifi_env  # noqa: F401

DHCPV6_PREFIX = "2001:db8:5"
DHCPV6_SRV_IP6 = f"{DHCPV6_PREFIX}::1"
DHCPV6_LEASE_PATH = "/tmp/nipart_test_dnsmasq_v6_wifi.lease"


def _start_dhcpv6_server():
    stop_dhcp_server()
    exec_cmd(["sudo", "rm", "-f", DHCPV6_LEASE_PATH])
    exec_cmd(
        f"ip netns exec {TEST_NET_NS} "
        f"ip addr del {DHCPV6_SRV_IP6}/64 dev {DHCP_SRV_NIC}".split(),
        check=False,
    )
    exec_cmd(
        f"ip netns exec {TEST_NET_NS} "
        f"ip addr add {DHCPV6_SRV_IP6}/64 dev {DHCP_SRV_NIC}".split()
    )
    exec_cmd(
        f"sudo ip netns exec {TEST_NET_NS} dnsmasq "
        f"--log-dhcp --conf-file=/dev/null "
        f"--dhcp-leasefile={DHCPV6_LEASE_PATH} "
        f"--pid-file={DNSMASQ_PID_PATH} --no-hosts "
        f"--dhcp-host=dummy-host,{DHCP_SRV_IP4_PREFIX}.99 "
        f"--dhcp-option=option:dns-server,8.8.8.8,1.1.1.1 "
        f"--dhcp-option=option:mtu,1492 "
        f"--dhcp-option=option:domain-name,example.com "
        f"--dhcp-option=option:ntp-server,{DHCP_SRV_IP4} "
        f"--dhcp-option=option6:ntp-server,"
        f"ntp-a.example.com,ntp-b.example.com "
        f"--dhcp-option=121,203.0.113.0/24,{DHCP_SRV_IP4_PREFIX}.40 "
        f"--dhcp-option=249,203.0.113.0/24,{DHCP_SRV_IP4_PREFIX}.40 "
        f"--interface={DHCP_SRV_NIC} --enable-ra "
        f"--dhcp-range={DHCPV6_PREFIX}::2,{DHCPV6_PREFIX}::fff,ra-names,"
        f"slaac,64,2m "
        f"--dhcp-range={DHCP_SRV_IP4_PREFIX}.2,{DHCP_SRV_IP4_PREFIX}.50,2m "
        f"--no-ping".split()
    )


@pytest.fixture(scope="module")
def dhcpv6_server(wifi_env):  # noqa: F811
    _start_dhcpv6_server()
    yield


@pytest.fixture
def clean_up():
    yield
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {WIFI_TEST_NIC}
                type: wifi-phy
                state: absent"""))
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {TEST_WIFI_SSID}
                type: wifi-cfg
                state: absent"""))


def _has_dhcpv6_addr():
    rc, out, _ = exec_cmd(
        ["ip", "-6", "addr", "show", "dev", WIFI_TEST_NIC],
        check=False,
    )
    if rc != 0:
        return False
    return DHCPV6_PREFIX in out and "/128" in out


def _dhcpv6_state_done():
    iface_state = show_only(WIFI_TEST_NIC)
    if iface_state is None:
        return False
    ipv6_conf = iface_state.get("ipv6", {})
    return ipv6_conf.get("dhcp") is True and (
        ipv6_conf.get("dhcp-state") == "done"
    )


def _connected_ssid():
    rc, out, _ = exec_cmd(f"iw dev {WIFI_TEST_NIC} link".split(), check=False)
    if rc != 0:
        return None
    match = re.search(r"SSID: (.+)", out)
    return match.group(1) if match else None


@pytest.mark.skipif(
    not has_kernel_module("mac80211_hwsim"),
    reason="Does not have 'mac80211_hwsim' module",
)
class TestWifiDhcpV6:
    def test_wifi_phy_dhcpv6(self, clean_up, dhcpv6_server):  # noqa: F811
        nipart.apply(load_yaml(f"""---
                interfaces:
                  - name: {WIFI_TEST_NIC}
                    type: wifi-phy
                    state: up
                    wifi:
                      ssid: {TEST_WIFI_SSID}
                      password: {TEST_WIFI_PSK}
                    ipv6:
                      enabled: true
                      dhcp: true"""))
        assert retry_till_true_or_timeout(60, _has_dhcpv6_addr)
        assert retry_till_true_or_timeout(60, _dhcpv6_state_done)

    def test_wifi_cfg_dhcpv6_already_connected(
        self, clean_up, dhcpv6_server  # noqa: F811
    ):
        nipart.apply(load_yaml(f"""---
                interfaces:
                  - name: {TEST_WIFI_SSID}
                    type: wifi-cfg
                    state: up
                    wifi:
                      ssid: {TEST_WIFI_SSID}
                      password: {TEST_WIFI_PSK}
                      base-iface: {WIFI_TEST_NIC}"""))
        assert retry_till_true_or_timeout(
            60, lambda: _connected_ssid() == TEST_WIFI_SSID
        )
        nipart.apply(
            load_yaml(f"""---
                interfaces:
                  - name: {TEST_WIFI_SSID}
                    type: wifi-cfg
                    state: up
                    wifi:
                      ssid: {TEST_WIFI_SSID}
                      password: {TEST_WIFI_PSK}
                      base-iface: {WIFI_TEST_NIC}
                    ipv6:
                      enabled: true
                      dhcp: true"""),
            verify_change=False,
        )
        assert retry_till_true_or_timeout(60, _has_dhcpv6_addr)

    def test_wifi_cfg_dhcpv6(self, clean_up, dhcpv6_server):  # noqa: F811
        nipart.apply(load_yaml(f"""---
                interfaces:
                  - name: {TEST_WIFI_SSID}
                    type: wifi-cfg
                    state: up
                    wifi:
                      ssid: {TEST_WIFI_SSID}
                      password: {TEST_WIFI_PSK}
                      base-iface: {WIFI_TEST_NIC}
                    ipv6:
                      enabled: true
                      dhcp: true"""))
        assert retry_till_true_or_timeout(
            60, lambda: _connected_ssid() == TEST_WIFI_SSID
        )
        assert retry_till_true_or_timeout(60, _has_dhcpv6_addr)
