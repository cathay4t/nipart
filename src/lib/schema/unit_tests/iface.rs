// SPDX-License-Identifier: Apache-2.0

use crate::{
    Interface, InterfaceIdentifier, InterfaceState, InterfaceType,
    NipartInterface,
};

#[test]
fn test_iface_de_ethernet() {
    let iface: Interface = serde_yaml::from_str(
        r#"---
        name: eth1
        type: ethernet
        "#,
    )
    .unwrap();
    assert!(matches!(iface, Interface::Ethernet(_)));
    assert_eq!(iface.name(), "eth1");
}

#[test]
fn test_iface_de_veth_maps_to_ethernet() {
    let iface: Interface = serde_yaml::from_str(
        r#"---
        name: veth0
        type: veth
        veth:
          peer: veth1
        "#,
    )
    .unwrap();

    match &iface {
        Interface::Ethernet(e) => {
            assert_eq!(e.veth.as_ref().map(|v| v.peer.as_str()), Some("veth1"));
        }
        _ => panic!("veth should deserialize into Ethernet variant"),
    }
}

#[test]
fn test_iface_de_ovs_bridge() {
    let iface: Interface = serde_yaml::from_str(
        r#"---
        name: ovsbr0
        type: ovs-bridge
        "#,
    )
    .unwrap();
    assert!(matches!(iface, Interface::OvsBridge(_)));
}

#[test]
fn test_iface_de_ovs_interface() {
    let iface: Interface = serde_yaml::from_str(
        r#"---
        name: ovs0
        type: ovs-interface
        "#,
    )
    .unwrap();
    assert!(matches!(iface, Interface::OvsInterface(_)));
}

#[test]
fn test_iface_de_loopback() {
    let iface: Interface = serde_yaml::from_str(
        r#"---
        name: lo
        type: loopback
        "#,
    )
    .unwrap();
    assert!(matches!(iface, Interface::Loopback(_)));
}

#[test]
fn test_iface_de_wifi_phy() {
    let iface: Interface = serde_yaml::from_str(
        r#"---
        name: wlan0
        type: wifi-phy
        wifi:
          ssid: Test-WIFI
        "#,
    )
    .unwrap();
    assert!(matches!(iface, Interface::WifiPhy(_)));
}

#[test]
fn test_iface_de_wifi_cfg() {
    let iface: Interface = serde_yaml::from_str(
        r#"---
        name: Test-WIFI
        type: wifi-cfg
        wifi:
          ssid: Test-WIFI
          base-iface: wlan0
        "#,
    )
    .unwrap();
    assert!(matches!(iface, Interface::WifiCfg(_)));
}

#[test]
fn test_iface_de_dummy() {
    let iface: Interface = serde_yaml::from_str(
        r#"---
        name: dummy0
        type: dummy
        "#,
    )
    .unwrap();
    assert!(matches!(iface, Interface::Dummy(_)));
}

#[test]
fn test_iface_de_vlan() {
    let iface: Interface = serde_yaml::from_str(
        r#"---
        name: eth1.100
        type: vlan
        vlan:
          base-iface: eth1
          id: 100
        "#,
    )
    .unwrap();
    assert!(matches!(iface, Interface::Vlan(_)));
}

#[test]
fn test_iface_de_vxlan() {
    let iface: Interface = serde_yaml::from_str(
        r#"---
        name: vxlan0
        type: vxlan
        vxlan:
          base-iface: eth1
          id: 100
        "#,
    )
    .unwrap();
    assert!(matches!(iface, Interface::Vxlan(_)));
}

