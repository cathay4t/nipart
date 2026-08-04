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

