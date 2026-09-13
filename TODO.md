# TODO

- `wifi_test.py::test_wifi_off_scan_fails_and_up_restores` fails on
  VM: after `npt wifi off` and `npt wifi on`, ping works but
  `iw dev test-wlan0 link` reports no SSID, so the
  `connected_ssid() == TEST_WIFI_SSID` assertion fails. Reproduced with
  the monitor and DHCP fixes of 2026-09-13 reverted, hence unrelated.
- Restart DHCPv6 service upon link local address changes
- Support filtering full network query to a single interface
- OVS bridge
- MacSec
- HSR
- MacVlan
- Infiniband
- SRIOV
- IPSec
- IpVlan
- Plugin cannot send back logs to user
- `nmc wifi connect` should wait connect and retry for wrong-password
- Expose per-SSID wifi roaming config (`roaming` / `roaming-threshold`)
- in `WifiConfig` schema and pass through to shuli `NetworkConfig`
- (currently uses shuli defaults: roaming enabled at -70 dBm)
- `wifi_phy_later_test.py`: after a saved `wifi-cfg` is handed to the
  wifi plugin on a new-phy event, shuli's ongoing scan makes
  `hostapd_is_up_open()` fail with `iw scan` returning device busy. The
  test fails consistently on `dev` before hostapd can be verified.
- `wifi_hidden_test.py`: hidden-SSID apply can fail verification because
  the daemon still reads an empty SSID after shuli reports connected and
  hostapd completed the handshake.  The same test passes when run alone,
  but fails consistently when the full file runs.
- `wait-ip: no|any|ipv4|ipv6|ipv4+ipv6` for whether wait IP applied.
- NmPolicy support as nmstate does
