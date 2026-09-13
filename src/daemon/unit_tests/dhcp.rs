// SPDX-License-Identifier: Apache-2.0

use nipart::{
    Interface, MergedInterface, MergedNetworkState, NetworkState,
    NipartApplyOption,
};

use super::{
    desired_ssid_for_phy, should_touch_dhcp, wifi_cfg_ssid_changed,
    wifi_ssid_changed,
};

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
fn test_dhcp_touched_when_restart_auto_ip_ssid_or_ip_changed() {
    assert!(should_touch_dhcp(true, false, false, true));
    assert!(should_touch_dhcp(false, true, false, true));
    assert!(should_touch_dhcp(false, false, true, true));
}

#[test]
fn test_dhcp_touched_when_interface_down() {
    assert!(should_touch_dhcp(false, false, false, false));
}

fn wifi_cfg_profile(
    name: &str,
    ssid: &str,
    base_iface: Option<&str>,
) -> String {
    let base_iface_line = match base_iface {
        Some(base_iface) => format!("      base-iface: {base_iface}\n"),
        None => String::new(),
    };
    format!(
        "  - name: {name}
    type: wifi-cfg
    state: up
    ipv4:
      enabled: true
      dhcp: true
    wifi:
      ssid: {ssid}
{base_iface_line}"
    )
}

fn merge_wifi(desired_ifaces: &str, current_yaml: &str) -> MergedNetworkState {
    let desired: NetworkState = rmsd_yaml::from_str(&format!(
        "---
interfaces:
{desired_ifaces}"
    ))
    .unwrap();
    let current: NetworkState = rmsd_yaml::from_str(current_yaml).unwrap();
    MergedNetworkState::new(
        desired,
        current,
        None,
        NipartApplyOption::default(),
    )
    .unwrap()
}

fn current_wifi_phy(ssid: Option<&str>) -> String {
    let wifi = match ssid {
        Some(ssid) => format!("    wifi:\n      ssid: {ssid}\n"),
        None => String::new(),
    };
    format!(
        "---
interfaces:
  - name: wlan0
    type: wifi-phy
    state: up
    link-state: up
    mac-address: 02:00:00:00:00:01
{wifi}"
    )
}

fn merged_phy(merged: &MergedNetworkState) -> &MergedInterface {
    merged.ifaces.kernel_ifaces.get("wlan0").unwrap()
}

#[test]
fn test_wifi_cfg_ssid_changed_on_profile_switch() {
    // Applying a wifi-cfg profile while the phy is associated to another
    // SSID must be treated as an SSID change, otherwise the DHCP client
    // keeps renewing the previous network's lease.
    let merged = merge_wifi(
        &wifi_cfg_profile("Office-WIFI", "Office-WIFI", None),
        &current_wifi_phy(Some("Home-WIFI")),
    );
    assert!(wifi_cfg_ssid_changed(&merged.ifaces, merged_phy(&merged)));
    assert_eq!(
        desired_ssid_for_phy(&merged.ifaces, merged_phy(&merged)),
        Some("Office-WIFI".to_string())
    );
}

#[test]
fn test_wifi_cfg_ssid_changed_false_when_profile_matches_current() {
    let merged = merge_wifi(
        &wifi_cfg_profile("Office-WIFI", "Office-WIFI", None),
        &current_wifi_phy(Some("Office-WIFI")),
    );
    assert!(!wifi_cfg_ssid_changed(&merged.ifaces, merged_phy(&merged)));
}

#[test]
fn test_wifi_cfg_ssid_changed_true_when_current_ssid_unknown() {
    // The phy may be mid re-association: the profile SSID is known, so
    // the DHCP client must wait for it instead of renewing the lease of
    // the network which is being left.
    let merged = merge_wifi(
        &wifi_cfg_profile("Office-WIFI", "Office-WIFI", None),
        &current_wifi_phy(None),
    );
    assert!(wifi_cfg_ssid_changed(&merged.ifaces, merged_phy(&merged)));
}

#[test]
fn test_wifi_cfg_ssid_changed_false_for_multiple_ssids() {
    // The boot pass hands over all saved profiles: with several different
    // SSIDs there is no single target SSID, so a healthy lease must not
    // be torn down.
    let desired = format!(
        "{}{}",
        wifi_cfg_profile("Home-WIFI", "Home-WIFI", None),
        wifi_cfg_profile("Office-WIFI", "Office-WIFI", None)
    );
    let merged = merge_wifi(&desired, &current_wifi_phy(Some("Home-WIFI")));
    assert!(!wifi_cfg_ssid_changed(&merged.ifaces, merged_phy(&merged)));
    assert_eq!(
        desired_ssid_for_phy(&merged.ifaces, merged_phy(&merged)),
        None
    );
}

#[test]
fn test_wifi_cfg_ssid_changed_false_for_other_phy() {
    let merged = merge_wifi(
        &wifi_cfg_profile("Office-WIFI", "Office-WIFI", Some("wlan1")),
        &current_wifi_phy(Some("Home-WIFI")),
    );
    assert!(!wifi_cfg_ssid_changed(&merged.ifaces, merged_phy(&merged)));
}
