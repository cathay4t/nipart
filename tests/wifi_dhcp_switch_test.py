# SPDX-License-Identifier: Apache-2.0

import os
import re
import signal

import nipart
import pytest

from .testlib.cmdlib import exec_cmd
from .testlib.dhcp import DHCP_SRV_IP4
from .testlib.dhcp import DHCP_SRV_IP4_PREFIX
from .testlib.dhcp import DHCP_SRV_NIC
from .testlib.dhcp import DNSMASQ_CONF_PATH
from .testlib.dhcp import DNSMASQ_PID_PATH
from .testlib.dhcp import stop_dhcp_server
from .testlib.env import has_kernel_module
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml
from .testlib.wifi import AP2_NIC
from .testlib.wifi import HOSTAPD_CONF_PATH
from .testlib.wifi import HOSTAPD_CONF_PATH_2
from .testlib.wifi import HOSTAPD_PID_PATH
from .testlib.wifi import HOSTAPD_PID_PATH_2
from .testlib.wifi import HWSIM0_PERM_MAC
from .testlib.wifi import HWSIM1_PERM_MAC
from .testlib.wifi import HWSIM2_PERM_MAC
from .testlib.wifi import TEST_NET_NS
from .testlib.wifi import TEST_WIFI_PSK
from .testlib.wifi import TEST_WIFI_SSID
from .testlib.wifi import TEST_WIFI_SSID_2
from .testlib.wifi import TIMEOUT_SECS_SIM_WIFI_NICS
from .testlib.wifi import WIFI_TEST_NIC
from .testlib.wifi import get_nic_name_by_perm_mac
from .testlib.wifi import get_wifi_phy_name
from .testlib.wifi import hostapd_is_up_2
from .testlib.wifi import start_hostapd
from .testlib.wifi import unload_wifi_sim_kernel_module

DHCP_SRV_IP4_PREFIX_2 = "198.51.100"
DHCP_SRV_IP4_2 = f"{DHCP_SRV_IP4_PREFIX_2}.1"
DNSMASQ_CONF_PATH_2 = "/tmp/nipart_test_dnsmasq2.conf"
DNSMASQ_PID_PATH_2 = "/tmp/nipart_test_dnsmasq2.pid"
TEST_NET_NS_2 = "wifi-test-2"
HOSTAPD_CONF_2 = f"""
interface={AP2_NIC}
driver=nl80211

hw_mode=g
channel=6
ssid={TEST_WIFI_SSID_2}

wpa=0
auth_algs=1
"""


def _has_three_sim_wifi_nics():
    exec_cmd("udevadm settle".split())
    state = nipart.show()
    return all(
        get_nic_name_by_perm_mac(state, mac)
        for mac in (HWSIM0_PERM_MAC, HWSIM1_PERM_MAC, HWSIM2_PERM_MAC)
    )


def _start_dhcp_server_2():
    exec_cmd(
        f"ip netns exec {TEST_NET_NS_2} "
        f"ip addr add {DHCP_SRV_IP4_2}/24 dev {AP2_NIC}".split()
    )
    dnsmasq_conf = (
        "leasefile-ro\n"
        f"interface={AP2_NIC}\n"
        f"dhcp-range={DHCP_SRV_IP4_PREFIX_2}.200,"
        f"{DHCP_SRV_IP4_PREFIX_2}.250,255.255.255.0,48h\n"
        f"dhcp-option=option:dns-server,{DHCP_SRV_IP4_2}\n"
    )
    with open(DNSMASQ_CONF_PATH_2, "w") as fd:
        fd.write(dnsmasq_conf)
    exec_cmd(
        f"sudo ip netns exec {TEST_NET_NS_2} dnsmasq "
        f"--interface={AP2_NIC} --log-dhcp "
        f"--pid-file={DNSMASQ_PID_PATH_2} "
        f"--conf-file={DNSMASQ_CONF_PATH_2} ".split()
    )


def _start_dhcp_server():
    exec_cmd(
        f"ip netns exec {TEST_NET_NS} "
        f"ip addr add {DHCP_SRV_IP4}/24 dev {DHCP_SRV_NIC}".split()
    )
    dnsmasq_conf = (
        "leasefile-ro\n"
        f"interface={DHCP_SRV_NIC}\n"
        f"dhcp-range={DHCP_SRV_IP4_PREFIX}.200,"
        f"{DHCP_SRV_IP4_PREFIX}.250,255.255.255.0,48h\n"
        f"dhcp-option=option:dns-server,{DHCP_SRV_IP4}\n"
    )
    with open(DNSMASQ_CONF_PATH, "w") as fd:
        fd.write(dnsmasq_conf)
    exec_cmd(
        f"sudo ip netns exec {TEST_NET_NS} dnsmasq "
        f"--interface={DHCP_SRV_NIC} --bind-interfaces --log-dhcp "
        f"--pid-file={DNSMASQ_PID_PATH} "
        f"--conf-file={DNSMASQ_CONF_PATH} ".split()
    )


def _stop_dhcp_server_2():
    if not os.path.exists(DNSMASQ_PID_PATH_2):
        return
    with open(DNSMASQ_PID_PATH_2) as fd:
        try:
            os.kill(int(fd.read()), signal.SIGTERM)
        except (ProcessLookupError, ValueError):
            pass


def _pid_alive(pid_path):
    if not os.path.exists(pid_path):
        return False
    with open(pid_path) as fd:
        pid = fd.read().strip()
    if not pid:
        return False
    try:
        with open(f"/proc/{pid}/stat") as fd:
            state = fd.read().split()[2]
        return state != "Z"
    except (FileNotFoundError, ProcessLookupError, ValueError):
        return False


def _dhcp_server_1_running():
    return _pid_alive(DNSMASQ_PID_PATH)


def _dhcp_server_2_running():
    return _pid_alive(DNSMASQ_PID_PATH_2)


def _start_hostapd_2():
    phy_id = get_wifi_phy_name(AP2_NIC)
    assert phy_id
    exec_cmd(f"iw phy#{phy_id} set netns name {TEST_NET_NS_2}".split())
    exec_cmd(f"ip netns exec {TEST_NET_NS_2} ip link set {AP2_NIC} up".split())
    with open(HOSTAPD_CONF_PATH_2, "w") as fd:
        fd.write(HOSTAPD_CONF_2)
    exec_cmd(
        f"ip netns exec {TEST_NET_NS_2} "
        f"hostapd -B -d {HOSTAPD_CONF_PATH_2} "
        f"-P {HOSTAPD_PID_PATH_2}".split(),
    )
    assert retry_till_true_or_timeout(2, hostapd_is_up_2)


@pytest.fixture(scope="module")
def two_dhcp_ap_env():
    exec_cmd("modprobe -r mac80211_hwsim".split(), check=False)
    exec_cmd(f"ip netns del {TEST_NET_NS}".split(), check=False)
    exec_cmd(f"ip netns del {TEST_NET_NS_2}".split(), check=False)
    exec_cmd(f"ip netns add {TEST_NET_NS}".split())
    exec_cmd(f"ip netns add {TEST_NET_NS_2}".split())

    exec_cmd("modprobe mac80211_hwsim radios=3".split())
    assert retry_till_true_or_timeout(
        TIMEOUT_SECS_SIM_WIFI_NICS, _has_three_sim_wifi_nics
    )

    state = nipart.show()
    exec_cmd("killall wpa_supplicant".split(), check=False)
    wlan0 = get_nic_name_by_perm_mac(state, HWSIM0_PERM_MAC)
    exec_cmd(f"ip link set {wlan0} name {WIFI_TEST_NIC}".split())
    wlan1 = get_nic_name_by_perm_mac(state, HWSIM1_PERM_MAC)
    exec_cmd(f"ip link set {wlan1} name {DHCP_SRV_NIC}".split())
    wlan2 = get_nic_name_by_perm_mac(state, HWSIM2_PERM_MAC)
    exec_cmd(f"ip link set {wlan2} name {AP2_NIC}".split())

    start_hostapd(with_dhcp=False)
    _start_hostapd_2()
    _start_dhcp_server()
    _start_dhcp_server_2()
    assert retry_till_true_or_timeout(5, _dhcp_server_2_running)
    yield

    _stop_dhcp_server_2()
    stop_dhcp_server()
    for pid_path in (HOSTAPD_PID_PATH, HOSTAPD_PID_PATH_2):
        if os.path.exists(pid_path):
            with open(pid_path) as fd:
                pid = fd.read()
            if pid:
                os.kill(int(pid), signal.SIGTERM)
    for conf_path in (HOSTAPD_CONF_PATH, HOSTAPD_CONF_PATH_2):
        if os.path.exists(conf_path):
            os.remove(conf_path)
    exec_cmd(f"ip netns del {TEST_NET_NS}".split(), check=False)
    exec_cmd(f"ip netns del {TEST_NET_NS_2}".split(), check=False)
    retry_till_true_or_timeout(10, unload_wifi_sim_kernel_module)


@pytest.fixture
def clean_up():
    yield
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {WIFI_TEST_NIC}
                type: wifi-phy
                state: absent"""))


def _connected_ssid():
    rc, out, _ = exec_cmd(f"iw dev {WIFI_TEST_NIC} link".split(), check=False)
    if rc != 0:
        return None
    match = re.search(r"SSID: (.+)", out)
    return match.group(1) if match else None


def _ipv4_addrs():
    rc, out, _ = exec_cmd(
        ["ip", "-4", "-o", "addr", "show", "dev", WIFI_TEST_NIC],
        check=False,
    )
    if rc != 0:
        return []
    addrs = []
    for line in out.splitlines():
        parts = line.split()
        if len(parts) >= 4 and parts[2] == "inet":
            addrs.append(parts[3].split("/")[0])
    return addrs


def _has_ipv4_prefix(prefix):
    return any(addr.startswith(f"{prefix}.") for addr in _ipv4_addrs())


@pytest.mark.skipif(
    not has_kernel_module("mac80211_hwsim"),
    reason="Does not have 'mac80211_hwsim' kernel module",
)
class TestWifiDhcpSwitch:
    def test_switch_ssid_purges_previous_dhcp(
        self, clean_up, two_dhcp_ap_env  # noqa: F811
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
                      dhcp: true"""))
        assert retry_till_true_or_timeout(
            60, lambda: _connected_ssid() == TEST_WIFI_SSID
        )
        assert retry_till_true_or_timeout(
            60, lambda: _has_ipv4_prefix(DHCP_SRV_IP4_PREFIX)
        )

        stop_dhcp_server()
        exec_cmd(
            ["sudo", "pkill", "-9", "-f", "nipart_test_dnsmasq.conf"],
            check=False,
        )
        assert not _dhcp_server_1_running()
        nipart.apply(load_yaml(f"""---
                interfaces:
                  - name: {WIFI_TEST_NIC}
                    type: wifi-phy
                    state: up
                    wifi:
                      ssid: {TEST_WIFI_SSID_2}
                    ipv4:
                      enabled: true
                      dhcp: true"""))
        assert retry_till_true_or_timeout(
            60, lambda: _connected_ssid() == TEST_WIFI_SSID_2
        )
        assert retry_till_true_or_timeout(
            60, lambda: not _has_ipv4_prefix(DHCP_SRV_IP4_PREFIX)
        ), "previous DHCP address was not purged after SSID switch"
