// SPDX-License-Identifier: Apache-2.0

use nipart::Interface;

use super::{should_touch_dhcp, wifi_ssid_changed};

fn wifi_phy(ssid: Option<&str>) -> Interface {
    let yaml = match ssid {
        Some(ssid) => format!(
            "---
            name: wlan0
            type: wifi-phy
            state: up
            wifi:
              ssid: {ssid}
            "
        ),
        None => "---
            name: wlan0
            type: wifi-phy
            state: up
            "
        .to_string(),
    };
    rmsd_yaml::from_str(&yaml).unwrap()
}

#[test]
fn test_wifi_ssid_changed_false_when_desired_has_no_ssid() {
    // The link-up event path applies a wifi-phy synthesized from a
    // wifi-cfg.  It carries the IP config but intentionally no `wifi`
    // section, so it must not be treated as an SSID change.
    let current = wifi_phy(Some("Home-SSID"));
    let desired = wifi_phy(None);
    assert!(!wifi_ssid_changed(Some(&current), Some(&desired)));
}

#[test]
fn test_wifi_ssid_changed_false_when_ssid_unchanged() {
    let current = wifi_phy(Some("Home-SSID"));
    let desired = wifi_phy(Some("Home-SSID"));
    assert!(!wifi_ssid_changed(Some(&current), Some(&desired)));
}

#[test]
fn test_wifi_ssid_changed_true_when_ssid_differs() {
    let current = wifi_phy(Some("Home-SSID"));
    let desired = wifi_phy(Some("Office-SSID"));
    assert!(wifi_ssid_changed(Some(&current), Some(&desired)));
}

#[test]
fn test_wifi_ssid_changed_preserves_unknown_current_ssid_case() {
    // When the current association is not reported yet but the apply
    // explicitly requests an SSID, DHCP must still wait for that SSID.
    let current = wifi_phy(None);
    let desired = wifi_phy(Some("Home-SSID"));
    assert!(wifi_ssid_changed(Some(&current), Some(&desired)));
}

#[test]
fn test_wifi_ssid_changed_false_for_non_wifi_or_missing_interface() {
    let current = wifi_phy(Some("Home-SSID"));
    let desired = wifi_phy(Some("Office-SSID"));
    assert!(!wifi_ssid_changed(None, Some(&desired)));
    assert!(!wifi_ssid_changed(Some(&current), None));
    assert!(!wifi_ssid_changed(None, None));
}

#[test]
fn test_saved_only_diff_does_not_touch_running_dhcp() {
    // The link-up event synthesizes a wifi-phy whose only diff is a
    // saved-only property (profile-name); DHCP settings are unchanged, so
    // the healthy DHCP client must be left alone.
    assert!(!should_touch_dhcp(false, false, false, true));
}

#[test]
fn test_dhcp_touched_when_force_ssid_or_ip_changed() {
    assert!(should_touch_dhcp(true, false, false, true));
    assert!(should_touch_dhcp(false, true, false, true));
    assert!(should_touch_dhcp(false, false, true, true));
}

#[test]
fn test_dhcp_touched_when_interface_down() {
    assert!(should_touch_dhcp(false, false, false, false));
}
