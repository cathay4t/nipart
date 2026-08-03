// SPDX-License-Identifier: Apache-2.0

use crate::{InterfaceAutoConnect, NetworkState, NipartInterface};

#[test]
fn test_auto_connect_bool_true() {
    let state = NetworkState::new_from_yaml(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            state: up
            auto-connect: true
        "#,
    )
    .unwrap();

    let iface = state.ifaces.kernel_ifaces.get("eth1").unwrap();
    assert_eq!(
        iface.base_iface().auto_connect,
        Some(InterfaceAutoConnect::AutoConnect)
    );
}

#[test]
fn test_auto_connect_bool_false() {
    let state = NetworkState::new_from_yaml(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            state: up
            auto-connect: false
        "#,
    )
    .unwrap();

    let iface = state.ifaces.kernel_ifaces.get("eth1").unwrap();
    assert_eq!(
        iface.base_iface().auto_connect,
        Some(InterfaceAutoConnect::Manual)
    );
}

#[test]
fn test_auto_connect_stringified_bool() {
    let state = NetworkState::new_from_yaml(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            state: up
            auto-connect: "true"
          - name: eth2
            type: ethernet
            state: up
            auto-connect: "no"
        "#,
    )
    .unwrap();

    let iface = state.ifaces.kernel_ifaces.get("eth1").unwrap();
    assert_eq!(
        iface.base_iface().auto_connect,
        Some(InterfaceAutoConnect::AutoConnect)
    );
    let iface = state.ifaces.kernel_ifaces.get("eth2").unwrap();
    assert_eq!(
        iface.base_iface().auto_connect,
        Some(InterfaceAutoConnect::Manual)
    );
}

#[test]
fn test_auto_connect_wifi() {
    let state = NetworkState::new_from_yaml(
        r#"---
        interfaces:
          - name: wg0
            type: wireguard
            state: up
            auto-connect:
              wifi: HomeWifi
          - name: wg1
            type: wireguard
            state: up
            auto-connect:
              wifi-not: OfficeWifi
        "#,
    )
    .unwrap();

    let iface = state.ifaces.kernel_ifaces.get("wg0").unwrap();
    assert_eq!(
        iface.base_iface().auto_connect,
        Some(InterfaceAutoConnect::Wifi(Box::new("HomeWifi".to_string())))
    );
    let iface = state.ifaces.kernel_ifaces.get("wg1").unwrap();
    assert_eq!(
        iface.base_iface().auto_connect,
        Some(InterfaceAutoConnect::WifiNot(Box::new(
            "OfficeWifi".to_string()
        )))
    );
}

#[test]
fn test_auto_connect_yaml_round_trip() {
    let state = NetworkState::new_from_yaml(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            state: up
            auto-connect: true
          - name: eth2
            type: ethernet
            state: up
            auto-connect: false
          - name: wg0
            type: wireguard
            state: up
            auto-connect:
              wifi-not: HomeWifi
        "#,
    )
    .unwrap();

    let serialized = serde_yaml::to_string(&state).unwrap();
    let reparsed = NetworkState::new_from_yaml(&serialized).unwrap();
    assert_eq!(state, reparsed);
}

#[test]
fn test_auto_connect_invalid_string() {
    // The old `trigger` values are not valid for `auto-connect`.
    let result = NetworkState::new_from_yaml(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            state: up
            auto-connect: carrier
        "#,
    );
    assert!(result.is_err());
}

#[test]
fn test_auto_connect_invalid_map_key() {
    let result = NetworkState::new_from_yaml(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            state: up
            auto-connect:
              foo: bar
        "#,
    );
    assert!(result.is_err());
}

#[test]
fn test_old_trigger_property_rejected() {
    let result = NetworkState::new_from_yaml(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            state: up
            trigger: carrier
        "#,
    );
    assert!(result.is_err());
}
