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
#[test]
fn test_resolve_mac_identifier_perm_mac() {
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
          mac-address: 00:00:00:00:00:00
          permanent-mac-address: 00:23:45:67:89:1a
        "#,
    )
    .unwrap();

    desired.resolve_mac_identifier(&current).unwrap();

    let resolved = desired.kernel_ifaces.get("eth0").unwrap();
    assert_eq!(resolved.base_iface().kernel_iface_name.as_str(), "eth0");
    assert_eq!(resolved.base_iface().profile_name.as_deref(), Some("wan0"));
}

/// Test error when MAC does not match any current interface.
#[test]
fn test_resolve_mac_identifier_no_match() {
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
          mac-address: 00:00:00:00:00:00
        "#,
    )
    .unwrap();

    let result = desired.resolve_mac_identifier(&current);
    assert!(result.is_err());
}

/// Test re-resolution when profile_name already set and NIC name changed.
#[test]
fn test_resolve_mac_identifier_re_resolve() {
    let mut desired: Interfaces = serde_yaml::from_str(
        r#"---
        - name: eth0
          type: ethernet
          identifier: mac-address
          mac-address: 00:23:45:67:89:1a
          profile-name: wan0
        "#,
    )
    .unwrap();

    let current: Interfaces = serde_yaml::from_str(
        r#"---
        - name: enp0s3
          type: ethernet
          mac-address: 00:23:45:67:89:1a
        "#,
    )
    .unwrap();

    desired.resolve_mac_identifier(&current).unwrap();

    // name should be updated to new kernel name
    assert!(desired.kernel_ifaces.contains_key("enp0s3"));
    assert!(!desired.kernel_ifaces.contains_key("eth0"));
    let resolved = desired.kernel_ifaces.get("enp0s3").unwrap();
    assert_eq!(resolved.base_iface().name, "enp0s3");
    // kernel_iface_name should be updated to new kernel name
    assert_eq!(resolved.base_iface().kernel_iface_name.as_str(), "enp0s3");
    // profile_name should be preserved
    assert_eq!(resolved.base_iface().profile_name.as_deref(), Some("wan0"));
}

/// Test that absent interfaces are skipped.
#[test]
fn test_resolve_mac_identifier_absent_skipped() {
    let mut desired: Interfaces = serde_yaml::from_str(
        r#"---
        - name: wan0
          type: ethernet
          identifier: mac-address
          mac-address: 00:23:45:67:89:1a
          state: absent
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

    // Absent interface should NOT be resolved, but kernel_iface_name
    // should be set from name by push()
    assert!(desired.kernel_ifaces.contains_key("wan0"));
    assert!(!desired.kernel_ifaces.contains_key("eth0"));
    let resolved = desired.kernel_ifaces.get("wan0").unwrap();
    assert_eq!(resolved.base_iface().kernel_iface_name, "wan0");
}

/// Test resolving unknown interface type to the matched current type.
#[test]
fn test_resolve_mac_identifier_unknown_type() {
    let mut desired: Interfaces = serde_yaml::from_str(
        r#"---
        - name: wan0
          type: unknown
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

    let resolved = desired.kernel_ifaces.get("eth0").unwrap();
    // Should have resolved to Ethernet type
    assert_eq!(resolved.iface_type(), &InterfaceType::Ethernet);
    assert_eq!(resolved.base_iface().kernel_iface_name.as_str(), "eth0");
}

/// Test error when mac_address is not provided.
#[test]
fn test_resolve_mac_identifier_missing_mac() {
    let mut desired: Interfaces = serde_yaml::from_str(
        r#"---
        - name: wan0
          type: ethernet
          identifier: mac-address
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

    let result = desired.resolve_mac_identifier(&current);
    assert!(result.is_err());
}

/// Test that already-resolved interface (name matches kernel name) is skipped.
#[test]
fn test_resolve_mac_identifier_already_resolved() {
    let mut desired: Interfaces = serde_yaml::from_str(
        r#"---
        - name: eth0
          type: ethernet
          identifier: mac-address
          mac-address: 00:23:45:67:89:1a
          profile-name: wan0
          kernel-iface-name: eth0
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

    // Should still have eth0 (no changes)
    assert!(desired.kernel_ifaces.contains_key("eth0"));
    let resolved = desired.kernel_ifaces.get("eth0").unwrap();
    assert_eq!(resolved.base_iface().kernel_iface_name.as_str(), "eth0");
    assert_eq!(resolved.base_iface().profile_name.as_deref(), Some("wan0"));
}

/// Test case-insensitive MAC address matching.
#[test]
fn test_resolve_mac_identifier_case_insensitive() {
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
          mac-address: 00:23:45:67:89:1A
        "#,
    )
    .unwrap();

    desired.resolve_mac_identifier(&current).unwrap();

    let resolved = desired.kernel_ifaces.get("eth0").unwrap();
    assert_eq!(resolved.base_iface().kernel_iface_name.as_str(), "eth0");
    assert_eq!(resolved.base_iface().profile_name.as_deref(), Some("wan0"));
}

/// Test resolving multiple interfaces with MAC identifiers.
#[test]
fn test_resolve_mac_identifier_multiple_ifaces() {
    let mut desired: Interfaces = serde_yaml::from_str(
        r#"---
        - name: wan0
          type: ethernet
          identifier: mac-address
          mac-address: 00:11:22:33:44:55
        - name: wan1
          type: ethernet
          identifier: mac-address
          mac-address: aa:bb:cc:dd:ee:ff
        "#,
    )
    .unwrap();

    let current: Interfaces = serde_yaml::from_str(
        r#"---
        - name: eth0
          type: ethernet
          mac-address: 00:11:22:33:44:55
        - name: eth1
          type: ethernet
          mac-address: aa:bb:cc:dd:ee:ff
        "#,
    )
    .unwrap();

    desired.resolve_mac_identifier(&current).unwrap();

    let resolved1 = desired.kernel_ifaces.get("eth0").unwrap();
    assert_eq!(resolved1.base_iface().kernel_iface_name.as_str(), "eth0");
    assert_eq!(resolved1.base_iface().profile_name.as_deref(), Some("wan0"));
    let resolved2 = desired.kernel_ifaces.get("eth1").unwrap();
    assert_eq!(resolved2.base_iface().kernel_iface_name.as_str(), "eth1");
    assert_eq!(resolved2.base_iface().profile_name.as_deref(), Some("wan1"));
}

/// Test that permanent_mac_address is preferred over mac_address when matching.
#[test]
fn test_resolve_mac_identifier_perm_mac_preferred() {
    let mut desired: Interfaces = serde_yaml::from_str(
        r#"---
        - name: wan0
          type: ethernet
          identifier: mac-address
          mac-address: 00:11:22:33:44:55
        "#,
    )
    .unwrap();

    let current: Interfaces = serde_yaml::from_str(
        r#"---
        - name: eth0
          type: ethernet
          mac-address: ff:ff:ff:ff:ff:ff
          permanent-mac-address: 00:11:22:33:44:55
        - name: eth1
          type: ethernet
          mac-address: 00:11:22:33:44:55
        "#,
    )
    .unwrap();

    desired.resolve_mac_identifier(&current).unwrap();

    // Should match eth0 (permanent_mac_address match) not eth1
    assert!(desired.kernel_ifaces.contains_key("eth0"));
    assert!(!desired.kernel_ifaces.contains_key("eth1"));
    let resolved = desired.kernel_ifaces.get("eth0").unwrap();
    assert_eq!(resolved.base_iface().name, "eth0");
    assert_eq!(resolved.base_iface().kernel_iface_name.as_str(), "eth0");
    assert_eq!(resolved.base_iface().profile_name.as_deref(), Some("wan0"));
}

/// Test when multiple NICs hold the same MAC address (first match wins).
#[test]
fn test_resolve_mac_identifier_duplicate_mac_across_nics() {
    let mut desired: Interfaces = serde_yaml::from_str(
        r#"---
        - name: wan0
          type: ethernet
          identifier: mac-address
          mac-address: 00:11:22:33:44:55
        "#,
    )
    .unwrap();

    let current: Interfaces = serde_yaml::from_str(
        r#"---
        - name: eth0
          type: ethernet
          mac-address: 00:11:22:33:44:55
        - name: eth1
          type: ethernet
          mac-address: 00:11:22:33:44:55
        "#,
    )
    .unwrap();

    desired.resolve_mac_identifier(&current).unwrap();

    // Exactly one interface should match (first found in HashMap iteration)
    let matched = if desired.kernel_ifaces.contains_key("eth0") {
        "eth0"
    } else if desired.kernel_ifaces.contains_key("eth1") {
        "eth1"
    } else {
        panic!("No interface matched");
    };
    assert!(["eth0", "eth1"].contains(&matched));
    assert!(!desired.kernel_ifaces.contains_key("wan0"));
    let resolved = desired.kernel_ifaces.get(matched).unwrap();
    assert_eq!(resolved.base_iface().name, matched);
    assert_eq!(resolved.base_iface().kernel_iface_name.as_str(), matched);
    assert_eq!(resolved.base_iface().profile_name.as_deref(), Some("wan0"));
}

/// Test re-resolution when NIC renamed (eth0 -> eth1).
