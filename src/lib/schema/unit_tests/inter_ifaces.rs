// SPDX-License-Identifier: Apache-2.0

use crate::{
    BaseInterface, Interface, InterfaceIdentifier, InterfaceType, Interfaces,
    MergedNetworkState, NetworkState, NipartInterface,
};

/// Test basic MAC address matching with MAC provided.
#[test]
fn test_resolve_mac_identifier_basic_with_mac() {
    let mut desired: Interfaces = serde_yaml::from_str(
        r#"---
        - name: wan0
          type: ethernet
          identifier: mac-address
          mac-address: 00:23:45:67:89:1a
        "#,
    )
    .unwrap();

    let current: Interfaces = serde_yaml::from_str(
        r#"---
        - name: eth0
          type: ethernet
          mac-address: 00:23:45:67:89:1a
        "#,
    )
    .unwrap();

    desired.resolve_mac_identifier(&current).unwrap();

    // Interface key should be the kernel name (renamed for merge compat)
    assert!(desired.kernel_ifaces.contains_key("eth0"));
    assert!(!desired.kernel_ifaces.contains_key("wan0"));
    let resolved = desired.kernel_ifaces.get("eth0").unwrap();
    assert_eq!(resolved.base_iface().name, "eth0");
    assert_eq!(resolved.base_iface().kernel_iface_name.as_str(), "eth0");
    assert_eq!(resolved.base_iface().profile_name.as_deref(), Some("wan0"));
}

/// Test MAC address matching with `permanent-mac-address`.
