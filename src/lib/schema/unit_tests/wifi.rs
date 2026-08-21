// SPDX-License-Identifier: Apache-2.0

use crate::{
    ErrorKind, Interface, InterfaceType, NetworkState, NipartInterface,
    WifiCfgInterface, WifiPhyInterface,
};

fn sanitize_wifi_phy(
    iface: &WifiPhyInterface,
    current: Option<&WifiPhyInterface>,
) -> Result<(), crate::NipartError> {
    let mut for_save = iface.clone();
    let mut for_apply = iface.clone();
    let mut for_verify = iface.clone();
    let mut merged = iface.clone();
    iface.sanitize(
        current,
        &mut for_save,
        &mut for_apply,
        &mut for_verify,
        &mut merged,
    )
}

#[test]
fn test_wifi_phy_mac_identifier_supported() {
    let iface: WifiPhyInterface = rmsd_yaml::from_str(
        r#"---
        name: HomeWiFi
        type: wifi-phy
        identifier: mac-address
        mac-address: 02:00:00:00:00:10
        state: up
        "#,
    )
    .unwrap();

    assert!(sanitize_wifi_phy(&iface, None).is_ok());
}

#[test]
fn test_wifi_phy_hold_wifi_cfg_with_other_base_iface() {
    let iface: WifiPhyInterface = rmsd_yaml::from_str(
        r#"---
        name: wlan0
        type: wifi-phy
        wifi:
          ssid: Test-WIFI
          password: 12345678
          base-iface: wlan1
        "#,
    )
    .unwrap();

    let result = sanitize_wifi_phy(&iface, None);
    assert!(result.is_err());

    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
        assert!(e.msg.contains("wlan1"));
    }
}

#[test]
fn test_wifi_cfg_hidden_round_trip() {
    let iface: WifiCfgInterface = rmsd_yaml::from_str(
        r#"---
        name: Test-WIFI-HIDDEN
        type: wifi-cfg
        state: up
        wifi:
          ssid: Test-WIFI-HIDDEN
          hidden: true
        "#,
    )
    .unwrap();
    assert!(iface.wifi.as_ref().unwrap().hidden);

    let yaml = rmsd_yaml::to_string(&iface).unwrap();
    let parsed: WifiCfgInterface = rmsd_yaml::from_str(&yaml).unwrap();
    assert!(parsed.wifi.as_ref().unwrap().hidden);
}

#[test]
fn test_wifi_cfg_sanitize_keeps_full_saved_config_on_diff_apply() {
    let desired: WifiCfgInterface = rmsd_yaml::from_str(
        r#"---
        name: Test-WIFI
        type: wifi-cfg
        state: up
        wifi:
          ssid: Test-WIFI
          password: <_hidden_>
          base-iface: wlan0
        "#,
    )
    .unwrap();
    let mut for_save: WifiCfgInterface = rmsd_yaml::from_str(
        r#"---
        name: Test-WIFI
        type: wifi-cfg
        state: up
        wifi:
          ssid: Test-WIFI
          password: '12345678'
          base-iface: wlan0
        "#,
    )
    .unwrap();
    let mut for_apply: WifiCfgInterface = rmsd_yaml::from_str(
        r#"---
        name: Test-WIFI
        type: wifi-cfg
        state: up
        wifi:
          ssid: Test-WIFI
        "#,
    )
    .unwrap();
    let mut for_verify = desired.clone();
    let mut merged = for_save.clone();

    desired
        .sanitize(
            None,
            &mut for_save,
            &mut for_apply,
            &mut for_verify,
            &mut merged,
        )
        .unwrap();

    let wifi = for_save.wifi.as_ref().unwrap();
    assert_eq!(wifi.password.as_deref(), Some("12345678"));
    assert_eq!(wifi.base_iface.as_deref(), Some("wlan0"));
}

#[test]
fn test_wifi_phy_sanitize_keeps_full_saved_config_on_diff_apply() {
    let desired: WifiPhyInterface = rmsd_yaml::from_str(
        r#"---
        name: wlan0
        type: wifi-phy
        state: up
        wifi:
          ssid: Test-WIFI
          password: <_hidden_>
          base-iface: wlan0
        "#,
    )
    .unwrap();
    let mut for_save: WifiPhyInterface = rmsd_yaml::from_str(
        r#"---
        name: wlan0
        type: wifi-phy
        state: up
        wifi:
          ssid: Test-WIFI
          password: '12345678'
          base-iface: wlan0
        "#,
    )
    .unwrap();
    let mut for_apply: WifiPhyInterface = rmsd_yaml::from_str(
        r#"---
        name: wlan0
        type: wifi-phy
        state: up
        wifi:
          ssid: Test-WIFI
        "#,
    )
    .unwrap();
    let mut for_verify = desired.clone();
    let mut merged = for_save.clone();

    desired
        .sanitize(
            None,
            &mut for_save,
            &mut for_apply,
            &mut for_verify,
            &mut merged,
        )
        .unwrap();

    let wifi = for_save.wifi.as_ref().unwrap();
    assert_eq!(wifi.password.as_deref(), Some("12345678"));
    assert_eq!(wifi.base_iface.as_deref(), Some("wlan0"));
}

fn gen_wifi_phy_state_with_password() -> NetworkState {
    let plain_yaml = r"---
        interfaces:
          - name: wlan0
            type: wifi-phy
            wifi:
              ssid: Test-WIFI
              password: '12345678'
            ipv4:
              enabled: true
              dhcp: false
              address:
                - ip: 192.0.2.99
                  prefix-length: 24";
    NetworkState::new_from_yaml(plain_yaml).unwrap()
}

fn gen_wifi_cfg_state_with_password() -> NetworkState {
    let plain_yaml = r"---
        interfaces:
          - name: Test-WIFI
            type: wifi-cfg
            wifi:
              ssid: Test-WIFI
              password: '12345678'
              base-iface: wlan0";
    NetworkState::new_from_yaml(plain_yaml).unwrap()
}

fn gen_wifi_cfg_state_with_hidden_password() -> NetworkState {
    let hidden_yaml = r"---
        interfaces:
          - name: Test-WIFI
            type: wifi-cfg
            wifi:
              ssid: Test-WIFI
              password: <_hidden_>
              base-iface: wlan0";
    NetworkState::new_from_yaml(hidden_yaml).unwrap()
}

fn gen_wifi_phy_state_with_hidden_password() -> NetworkState {
    let hidden_yaml = r"---
        interfaces:
          - name: wlan0
            type: wifi-phy
            wifi:
              ssid: Test-WIFI
              password: <_hidden_>
            ipv4:
              enabled: true
              dhcp: false
              address:
                - ip: 192.0.2.99
                  prefix-length: 24";
    NetworkState::new_from_yaml(hidden_yaml).unwrap()
}

#[test]
fn test_wifi_phy_password_in_hide_secrets() {
    let mut state = gen_wifi_phy_state_with_password();
    let secrets = state.extract_secrets().unwrap();

    let wifi_iface = state
        .ifaces
        .kernel_ifaces
        .get("wlan0")
        .and_then(|i| {
            if let Interface::WifiPhy(w) = i {
                w.wifi.as_ref()
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        wifi_iface.password.as_deref(),
        Some(NetworkState::HIDE_SECRET_STR)
    );

    let secrets_wifi = secrets
        .ifaces
        .kernel_ifaces
        .get("wlan0")
        .and_then(|i| {
            if let Interface::WifiPhy(w) = i {
                w.wifi.as_ref()
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(secrets_wifi.password.as_deref(), Some("12345678"));
}

#[test]
fn test_wifi_phy_password_in_merge() {
    let mut safe_state = gen_wifi_phy_state_with_hidden_password();
    let secret_state = gen_wifi_phy_state_with_password();

    safe_state.merge(&secret_state).unwrap();

    let wifi_iface = safe_state
        .ifaces
        .kernel_ifaces
        .get("wlan0")
        .and_then(|i| {
            if let Interface::WifiPhy(w) = i {
                w.wifi.as_ref()
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(wifi_iface.password.as_deref(), Some("12345678"));
}

#[test]
fn test_wifi_phy_hidden_password_in_merge_keeps_old_secret() {
    let mut safe_state = gen_wifi_phy_state_with_password();
    let hidden_state = gen_wifi_phy_state_with_hidden_password();

    safe_state.merge(&hidden_state).unwrap();

    let wifi_iface = safe_state
        .ifaces
        .kernel_ifaces
        .get("wlan0")
        .and_then(|i| {
            if let Interface::WifiPhy(w) = i {
                w.wifi.as_ref()
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(wifi_iface.password.as_deref(), Some("12345678"));
}

#[test]
fn test_wifi_phy_password_in_gen_diff() {
    let safe_state = gen_wifi_phy_state_with_hidden_password();
    let plain_state = gen_wifi_phy_state_with_password();

    let diff_state = plain_state.gen_diff(&safe_state).unwrap();

    let diff_wifi = diff_state
        .ifaces
        .kernel_ifaces
        .get("wlan0")
        .and_then(|i| {
            if let Interface::WifiPhy(w) = i {
                w.wifi.as_ref()
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(diff_wifi.password.as_deref(), Some("12345678"));
}

#[test]
fn test_wifi_phy_hidden_password_in_gen_diff_no_diff() {
    let plain_state = gen_wifi_phy_state_with_password();
    let hidden_state = gen_wifi_phy_state_with_hidden_password();

    let diff_state = hidden_state.gen_diff(&plain_state).unwrap();

    assert!(
        !diff_state.ifaces.kernel_ifaces.contains_key("wlan0"),
        "Hidden password should not generate a diff, but got: {:?}",
        diff_state.ifaces.kernel_ifaces.get("wlan0")
    );
}

#[test]
fn test_wifi_cfg_password_in_hide_secrets() {
    let mut state = gen_wifi_cfg_state_with_password();
    let secrets = state.extract_secrets().unwrap();

    let wifi_iface = state
        .ifaces
        .user_ifaces
        .get(&("Test-WIFI".to_string(), InterfaceType::WifiCfg))
        .and_then(|i| {
            if let Interface::WifiCfg(w) = i {
                w.wifi.as_ref()
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        wifi_iface.password.as_deref(),
        Some(NetworkState::HIDE_SECRET_STR)
    );

    let secrets_wifi = secrets
        .ifaces
        .user_ifaces
        .get(&("Test-WIFI".to_string(), InterfaceType::WifiCfg))
        .and_then(|i| {
            if let Interface::WifiCfg(w) = i {
                w.wifi.as_ref()
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(secrets_wifi.password.as_deref(), Some("12345678"));
}

#[test]
fn test_wifi_cfg_password_in_merge() {
    let mut safe_state = gen_wifi_cfg_state_with_hidden_password();
    let secret_state = gen_wifi_cfg_state_with_password();

    safe_state.merge(&secret_state).unwrap();

    let wifi_iface = safe_state
        .ifaces
        .user_ifaces
        .get(&("Test-WIFI".to_string(), InterfaceType::WifiCfg))
        .and_then(|i| {
            if let Interface::WifiCfg(w) = i {
                w.wifi.as_ref()
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(wifi_iface.password.as_deref(), Some("12345678"));
}

#[test]
fn test_wifi_cfg_hidden_password_in_merge_keeps_old_secret() {
    let mut safe_state = gen_wifi_cfg_state_with_password();
    let hidden_state = gen_wifi_cfg_state_with_hidden_password();

    safe_state.merge(&hidden_state).unwrap();

    let wifi_iface = safe_state
        .ifaces
        .user_ifaces
        .get(&("Test-WIFI".to_string(), InterfaceType::WifiCfg))
        .and_then(|i| {
            if let Interface::WifiCfg(w) = i {
                w.wifi.as_ref()
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(wifi_iface.password.as_deref(), Some("12345678"));
}

#[test]
fn test_wifi_cfg_password_in_gen_diff() {
    let safe_state = gen_wifi_cfg_state_with_hidden_password();
    let plain_state = gen_wifi_cfg_state_with_password();

    let diff_state = plain_state.gen_diff(&safe_state).unwrap();

    let diff_wifi = diff_state
        .ifaces
        .user_ifaces
        .get(&("Test-WIFI".to_string(), InterfaceType::WifiCfg))
        .and_then(|i| {
            if let Interface::WifiCfg(w) = i {
                w.wifi.as_ref()
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(diff_wifi.password.as_deref(), Some("12345678"));
}

#[test]
fn test_wifi_cfg_hidden_password_in_gen_diff_no_diff() {
    let plain_state = gen_wifi_cfg_state_with_password();
    let hidden_state = gen_wifi_cfg_state_with_hidden_password();

    let diff_state = hidden_state.gen_diff(&plain_state).unwrap();

    assert!(
        !diff_state
            .ifaces
            .user_ifaces
            .contains_key(&("Test-WIFI".to_string(), InterfaceType::WifiCfg)),
        "Hidden password should not generate a diff, but got: {:?}",
        diff_state
            .ifaces
            .user_ifaces
            .get(&("Test-WIFI".to_string(), InterfaceType::WifiCfg))
    );
}

#[test]
fn test_extract_secrets_only_includes_changed_secrets() {
    let plain_yaml = r"---
        interfaces:
          - name: wlan0
            type: wifi-phy
            wifi:
              ssid: Test-WIFI
              password: '12345678'
            ipv4:
              enabled: true
              dhcp: false
              address:
                - ip: 192.0.2.99
                  prefix-length: 24
          - name: lan0
            type: ethernet
            state: up
            mac-address: 02:00:00:00:00:02
            mtu: 1500
            ipv4:
              enabled: true
              dhcp: true
              auto-gateway: false";
    let mut state = NetworkState::new_from_yaml(plain_yaml).unwrap();
    let secrets = state.extract_secrets().unwrap();

    // Ethernet interface should NOT be in secrets (no secrets to extract)
    assert!(
        !secrets.ifaces.kernel_ifaces.contains_key("lan0"),
        "Ethernet interface 'lan0' should not be in secrets, but found: {:?}",
        secrets.ifaces.kernel_ifaces.get("lan0")
    );

    // WiFi should have password in secrets
    let secrets_wifi = secrets
        .ifaces
        .kernel_ifaces
        .get("wlan0")
        .and_then(|i| {
            if let Interface::WifiPhy(w) = i {
                w.wifi.as_ref()
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(secrets_wifi.password.as_deref(), Some("12345678"));

    // Original state should have hidden password
    let hidden_wifi = state
        .ifaces
        .kernel_ifaces
        .get("wlan0")
        .and_then(|i| {
            if let Interface::WifiPhy(w) = i {
                w.wifi.as_ref()
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        hidden_wifi.password.as_deref(),
        Some(NetworkState::HIDE_SECRET_STR)
    );

    // Ethernet interface should still be in original state
    assert!(state.ifaces.kernel_ifaces.contains_key("lan0"));
}
