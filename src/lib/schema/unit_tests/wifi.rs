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

