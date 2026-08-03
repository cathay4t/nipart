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

