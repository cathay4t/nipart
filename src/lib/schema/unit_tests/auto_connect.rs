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
fn test_auto_connect_serialized_form() {
    // The round-trip test cannot catch a wrong serialized form since the
    // deserializer also accepts stringified booleans. Assert the exact
    // YAML value types here.
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
              wifi: HomeWifi
          - name: wg1
            type: wireguard
            state: up
            auto-connect:
              wifi-not: OfficeWifi
        "#,
    )
    .unwrap();

    let value = serde_yaml::to_value(&state).unwrap();
    let ifaces = value
        .get("interfaces")
        .and_then(serde_yaml::Value::as_sequence)
        .unwrap();
    assert_eq!(ifaces.len(), 4);
    for iface in ifaces {
        let name = iface
            .get("name")
            .and_then(serde_yaml::Value::as_str)
            .unwrap();
        let auto_connect = iface.get("auto-connect").unwrap();
        match name {
            "eth1" => {
                assert_eq!(auto_connect, &serde_yaml::Value::Bool(true));
            }
            "eth2" => {
                assert_eq!(auto_connect, &serde_yaml::Value::Bool(false));
            }
            "wg0" => {
                assert_eq!(auto_connect.as_mapping().unwrap().len(), 1);
                assert_eq!(
                    auto_connect.get("wifi"),
                    Some(&serde_yaml::Value::String("HomeWifi".to_string()))
                );
            }
            "wg1" => {
                assert_eq!(auto_connect.as_mapping().unwrap().len(), 1);
                assert_eq!(
                    auto_connect.get("wifi-not"),
                    Some(&serde_yaml::Value::String("OfficeWifi".to_string()))
                );
            }
            _ => panic!("Unexpected interface {name}"),
        }
    }
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

#[test]
fn test_process_auto_connect_match_by_mac_when_name_changed() {
    // A MAC-identified interface should still match the link event via MAC
    // address even when the kernel interface name changed after replug.
    let saved = NetworkState::new_from_yaml(
        r#"---
        interfaces:
          - name: lan0
            type: ethernet
            state: up
            identifier: mac-address
            mac-address: 02:00:00:00:00:02
            auto-connect: true
        "#,
    )
    .unwrap();
    let iface = saved.ifaces.kernel_ifaces.get("lan0").unwrap();

    let current = NetworkState::new_from_yaml(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            state: up
            mac-address: 02:00:00:00:00:02
        "#,
    )
    .unwrap();

    let event = crate::InterfaceLinkEvent::new(
        "eth1".to_string(),
        18,
        crate::InterfaceType::Ethernet,
        true,
        None,
    );
    assert_eq!(
        iface.process_auto_connect(&event, &current.ifaces),
        Some(true)
    );
}

#[test]
fn test_process_auto_connect_no_match_when_mac_differs() {
    let saved = NetworkState::new_from_yaml(
        r#"---
        interfaces:
          - name: lan0
            type: ethernet
            state: up
            identifier: mac-address
            mac-address: 02:00:00:00:00:02
            auto-connect: true
        "#,
    )
    .unwrap();
    let iface = saved.ifaces.kernel_ifaces.get("lan0").unwrap();

    let current = NetworkState::new_from_yaml(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            state: up
            mac-address: 00:11:22:33:44:55
        "#,
    )
    .unwrap();

    let event = crate::InterfaceLinkEvent::new(
        "eth1".to_string(),
        18,
        crate::InterfaceType::Ethernet,
        true,
        None,
    );
    assert_eq!(iface.process_auto_connect(&event, &current.ifaces), None);
}

#[test]
fn test_process_auto_connect_manual_ignores_event() {
    let saved = NetworkState::new_from_yaml(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            state: up
            auto-connect: false
        "#,
    )
    .unwrap();
    let iface = saved.ifaces.kernel_ifaces.get("eth1").unwrap();

    let event = crate::InterfaceLinkEvent::new(
        "eth1".to_string(),
        18,
        crate::InterfaceType::Ethernet,
        true,
        None,
    );
    assert_eq!(iface.process_auto_connect(&event, &saved.ifaces), None);
}
