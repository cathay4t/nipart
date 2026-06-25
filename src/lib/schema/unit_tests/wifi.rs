// SPDX-License-Identifier: Apache-2.0

use crate::{
    ErrorKind, Interface, InterfaceType, NetworkState, NipartInterface,
    WifiPhyInterface,
};

#[test]
fn test_wifi_phy_hold_wifi_cfg_with_other_base_iface() {
    let mut iface: WifiPhyInterface = serde_yaml::from_str(
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

    let result = iface.sanitize(None);
    assert!(result.is_err());

    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
        assert!(e.msg.contains("wlan1"));
    }
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
    let secrets = state.hide_secrets();

    let wifi_iface = state
        .ifaces
        .get("wlan0", Some(&InterfaceType::WifiPhy))
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
        .get("wlan0", Some(&InterfaceType::WifiPhy))
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
        .get("wlan0", Some(&InterfaceType::WifiPhy))
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
        .get("wlan0", Some(&InterfaceType::WifiPhy))
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

