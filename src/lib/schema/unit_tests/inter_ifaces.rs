// SPDX-License-Identifier: Apache-2.0

use crate::{
    BaseInterface, Interface, InterfaceIdentifier, InterfaceIpv4,
    InterfaceType, Interfaces, MergedInterface, MergedInterfaces,
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

/// Test that matching against multiple NICs with the same MAC raises error.
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

    let result = desired.resolve_mac_identifier(&current);
    assert!(result.is_err());
    assert!(result.unwrap_err().msg.contains("Multiple interfaces"));
}

/// Test re-resolution when NIC renamed (eth0 -> eth1).
#[test]
fn test_resolve_mac_identifier_re_resolve_type_change() {
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
        - name: eth1
          type: ethernet
          mac-address: 00:23:45:67:89:1a
        "#,
    )
    .unwrap();

    desired.resolve_mac_identifier(&current).unwrap();

    let resolved = desired.kernel_ifaces.get("eth1").unwrap();
    assert_eq!(resolved.base_iface().name, "eth1");
    assert_eq!(resolved.base_iface().kernel_iface_name.as_str(), "eth1");
    assert_eq!(resolved.base_iface().profile_name.as_deref(), Some("wan0"));
}

/// Test full merge flow with MAC identifier via MergedNetworkState.
#[test]
fn test_merge_flow_with_mac_identifier() {
    let desired: NetworkState = serde_yaml::from_str(
        r#"
        interfaces:
          - name: wan0
            type: ethernet
            state: up
            identifier: mac-address
            mac-address: 00:23:45:67:89:1a
            ipv4:
              enabled: true
              address:
                - ip: 192.168.1.100
                  prefix-length: 24
        "#,
    )
    .unwrap();

    let current: NetworkState = serde_yaml::from_str(
        r#"
        interfaces:
          - name: eth0
            type: ethernet
            mac-address: 00:23:45:67:89:1a
            state: up
        "#,
    )
    .unwrap();

    let merged =
        MergedNetworkState::new(desired, current, Default::default()).unwrap();
    let apply_state = merged.gen_state_for_apply();

    // After merge, the interface should be keyed by kernel name
    let apply_iface = apply_state.ifaces.kernel_ifaces.get("eth0").unwrap();
    assert_eq!(apply_iface.base_iface().name, "eth0");
    assert_eq!(apply_iface.base_iface().kernel_iface_name.as_str(), "eth0");
    assert_eq!(
        apply_iface.base_iface().profile_name.as_deref(),
        Some("wan0")
    );
    // IP config should have been merged
    assert!(apply_iface.base_iface().ipv4.is_some());
}

/// Test that kernel_iface_name() method works on Interface.
#[test]
fn test_kernel_iface_name_method() {
    let mut iface: Interface = serde_yaml::from_str(
        r#"---
        name: wan0
        type: ethernet
        "#,
    )
    .unwrap();

    // Without kernel_iface_name set, should return empty (before sanitize)
    assert!(iface.kernel_iface_name().is_empty());

    // After setting kernel_iface_name, should return it
    iface.base_iface_mut().kernel_iface_name = "eth0".to_string();
    assert_eq!(iface.kernel_iface_name(), "eth0");

    // After resolve, both name and kernel_iface_name are set
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
        "#,
    )
    .unwrap();
    desired.resolve_mac_identifier(&current).unwrap();
    let resolved = desired.kernel_ifaces.get("eth0").unwrap();
    assert_eq!(resolved.kernel_iface_name(), "eth0");
    assert_eq!(resolved.name(), "eth0");

    // When identifier is Name, resolve_name_identifier copies name to
    // kernel_iface_name
    let mut ifaces: Interfaces = serde_yaml::from_str(
        r#"---
        - name: eth1
          type: ethernet
        "#,
    )
    .unwrap();
    ifaces.resolve_name_identifier();
    let resolved = ifaces.kernel_ifaces.get("eth1").unwrap();
    assert_eq!(resolved.kernel_iface_name(), "eth1");
}

/// Test that sanitize does NOT override kernel_iface_name for MacAddress
/// identifier.
#[test]
fn test_sanitize_does_not_override_mac_kernel_iface_name() {
    let mut base =
        BaseInterface::new("wan0".to_string(), InterfaceType::Ethernet);
    base.identifier = Some(InterfaceIdentifier::MacAddress);
    base.kernel_iface_name = "eth0".to_string();
    base.sanitize(None).unwrap();
    // kernel_iface_name should be preserved (not overwritten by sanitize)
    assert_eq!(base.kernel_iface_name.as_str(), "eth0");
}

/// Test that route next-hop-interface with MAC identifier resolves from
/// logical name (profile_name) to kernel name.
#[test]
fn test_route_next_hop_iface_resolves_by_profile_name() {
    let desired: NetworkState = serde_yaml::from_str(
        r#"
        interfaces:
          - name: wan0
            type: ethernet
            state: up
            identifier: mac-address
            mac-address: 00:23:45:67:89:1a
        routes:
          config:
            - destination: 0.0.0.0/0
              next-hop-interface: wan0
              next-hop-address: 192.168.1.1
              table-id: 254
        "#,
    )
    .unwrap();

    let current: NetworkState = serde_yaml::from_str(
        r#"
        interfaces:
          - name: eth0
            type: ethernet
            mac-address: 00:23:45:67:89:1a
            state: up
            ipv4:
              enabled: true
              dhcp: false
        "#,
    )
    .unwrap();

    let merged =
        MergedNetworkState::new(desired, current, Default::default()).unwrap();

    let changed: Vec<&str> = merged
        .routes
        .changed_routes
        .iter()
        .filter_map(|r| r.next_hop_iface.as_deref())
        .collect();
    assert_eq!(changed, vec!["eth0"]);
}

/// Test that route next-hop-interface with direct kernel name succeeds
/// without profile_name lookup.
#[test]
fn test_route_next_hop_iface_direct_kernel_name() {
    let desired: NetworkState = serde_yaml::from_str(
        r#"
        interfaces:
          - name: wan0
            type: ethernet
            state: up
            identifier: mac-address
            mac-address: 00:23:45:67:89:1a
        routes:
          config:
            - destination: 0.0.0.0/0
              next-hop-interface: eth0
              next-hop-address: 192.168.1.1
              table-id: 254
        "#,
    )
    .unwrap();

    let current: NetworkState = serde_yaml::from_str(
        r#"
        interfaces:
          - name: eth0
            type: ethernet
            mac-address: 00:23:45:67:89:1a
            state: up
            ipv4:
              enabled: true
              dhcp: false
        "#,
    )
    .unwrap();

    let merged =
        MergedNetworkState::new(desired, current, Default::default()).unwrap();

    let changed: Vec<&str> = merged
        .routes
        .changed_routes
        .iter()
        .filter_map(|r| r.next_hop_iface.as_deref())
        .collect();
    assert_eq!(changed, vec!["eth0"]);
}

/// Test that route next-hop-interface pointing to absent interface
/// via logical name raises error.
#[test]
fn test_route_next_hop_iface_absent_by_logical_name() {
    let desired: NetworkState = serde_yaml::from_str(
        r#"
        interfaces:
          - name: wan0
            type: ethernet
            state: absent
            identifier: mac-address
            mac-address: 00:23:45:67:89:1a
        routes:
          config:
            - destination: 0.0.0.0/0
              next-hop-interface: wan0
              next-hop-address: 192.168.1.1
              table-id: 254
        "#,
    )
    .unwrap();

    let current: NetworkState = serde_yaml::from_str(
        r#"
        interfaces:
          - name: eth0
            type: ethernet
            mac-address: 00:23:45:67:89:1a
            state: up
        "#,
    )
    .unwrap();

    let result = MergedNetworkState::new(desired, current, Default::default());
    assert!(result.is_err());
    assert!(result.unwrap_err().msg.contains("marked as absent"));
}

/// Test that resolve_route_next_hop_iface returns error when multiple
/// interfaces share the same logical name (profile_name).
#[test]
fn test_route_next_hop_iface_duplicate_logical_name_error() {
    let mut kernel_ifaces = std::collections::HashMap::new();

    let mut iface1 = Interface::Ethernet(Box::new(
        serde_yaml::from_str(
            r#"---
            name: eth0
            type: ethernet
            profile-name: cunet
            "#,
        )
        .unwrap(),
    ));
    iface1.base_iface_mut().kernel_iface_name = "eth0".to_string();

    let mut iface2 = Interface::Ethernet(Box::new(
        serde_yaml::from_str(
            r#"---
            name: eth1
            type: ethernet
            profile-name: cunet
            "#,
        )
        .unwrap(),
    ));
    iface2.base_iface_mut().kernel_iface_name = "eth1".to_string();

    let merged1 = MergedInterface::new(Some(iface1), None).unwrap();
    let merged2 = MergedInterface::new(Some(iface2), None).unwrap();
    kernel_ifaces.insert("eth0".to_string(), merged1);
    kernel_ifaces.insert("eth1".to_string(), merged2);

    let merged_ifaces = MergedInterfaces {
        kernel_ifaces,
        ..Default::default()
    };

    let result = merged_ifaces.resolve_route_next_hop_iface("cunet");
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .msg
            .contains("matches multiple interfaces")
    );
}

/// Test that resolve_route_next_hop_iface returns the original name
/// when no match is found (neither kernel name nor profile_name).
#[test]
fn test_route_next_hop_iface_no_match_returns_original() {
    let merged_ifaces = MergedInterfaces::default();

    let result = merged_ifaces.resolve_route_next_hop_iface("nonexistent");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "nonexistent");
}

/// Test that resolve_route_next_hop_iface returns kernel name when
/// a single profile_name match is found.
#[test]
fn test_route_next_hop_iface_single_profile_match() {
    let mut kernel_ifaces = std::collections::HashMap::new();

    let mut iface = Interface::Ethernet(Box::new(
        serde_yaml::from_str(
            r#"---
            name: eth0
            type: ethernet
            profile-name: cunet
            "#,
        )
        .unwrap(),
    ));
    iface.base_iface_mut().kernel_iface_name = "eth0".to_string();

    let merged = MergedInterface::new(Some(iface), None).unwrap();
    kernel_ifaces.insert("eth0".to_string(), merged);

    let merged_ifaces = MergedInterfaces {
        kernel_ifaces,
        ..Default::default()
    };

    let result = merged_ifaces.resolve_route_next_hop_iface("cunet");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "eth0");
}

/// Test that route with next-hop-interface pointing to a non-existent
/// logical name still passes validation (the route is added but the
/// interface check is handled later by kernel).
#[test]
fn test_route_next_hop_iface_unmatched_logical_name_adds_route() {
    let desired: NetworkState = serde_yaml::from_str(
        r#"
        routes:
          config:
            - destination: 0.0.0.0/0
              next-hop-interface: nonexistent
              next-hop-address: 192.168.1.1
              table-id: 254
        "#,
    )
    .unwrap();

    let current = NetworkState::default();

    let merged =
        MergedNetworkState::new(desired, current, Default::default()).unwrap();

    assert!(!merged.routes.changed_routes.is_empty());
    assert_eq!(
        merged.routes.changed_routes[0].next_hop_iface.as_deref(),
        Some("nonexistent")
    );
}

/// Test parsing `auto-gateway` in IPv4 DHCP config.
#[test]
fn test_ipv4_auto_gateway_false() {
    let net_state: NetworkState = serde_yaml::from_str(
        r#"
        interfaces:
          - name: eth1
            type: ethernet
            state: up
            ipv4:
              enabled: true
              dhcp: true
              auto-gateway: false
        "#,
    )
    .unwrap();

    let iface = net_state
        .ifaces
        .kernel_ifaces
        .get("eth1")
        .map(|i| i.base_iface().ipv4.as_ref().unwrap().clone())
        .unwrap();

    assert_eq!(iface.auto_gateway, Some(false));
}

/// Test that default value for `auto-gateway` is `None` when not specified.
#[test]
fn test_ipv4_auto_gateway_defaults() {
    let net_state: NetworkState = serde_yaml::from_str(
        r#"
        interfaces:
          - name: eth1
            type: ethernet
            state: up
            ipv4:
              enabled: true
              dhcp: true
        "#,
    )
    .unwrap();

    let iface = net_state
        .ifaces
        .kernel_ifaces
        .get("eth1")
        .map(|i| i.base_iface().ipv4.as_ref().unwrap().clone())
        .unwrap();

    assert_eq!(iface.auto_gateway, None);
}

/// Test `auto-gateway: true`.
#[test]
fn test_ipv4_auto_gateway_true() {
    let net_state: NetworkState = serde_yaml::from_str(
        r#"
        interfaces:
          - name: eth1
            type: ethernet
            state: up
            ipv4:
              enabled: true
              dhcp: true
              auto-gateway: true
        "#,
    )
    .unwrap();

    let iface = net_state
        .ifaces
        .kernel_ifaces
        .get("eth1")
        .map(|i| i.base_iface().ipv4.as_ref().unwrap().clone())
        .unwrap();

    assert_eq!(iface.auto_gateway, Some(true));
}

/// Test that `InterfaceIpv4::new_disabled()` sets new fields to None.
#[test]
fn test_ipv4_new_disabled() {
    let ipv4 = InterfaceIpv4::new_disabled();
    assert_eq!(ipv4.auto_gateway, None);
}

/// Test that sanitize clears auto_gateway when DHCP is off.
#[test]
fn test_ipv4_sanitize_clears_when_dhcp_off() {
    let mut ipv4 = InterfaceIpv4 {
        enabled: Some(true),
        dhcp: Some(false),
        dhcp_state: None,
        addresses: None,
        auto_gateway: Some(false),
    };
    ipv4.sanitize(None).unwrap();
    assert_eq!(ipv4.auto_gateway, None);
}

/// Test that sanitize clears auto_gateway when IP disabled.
#[test]
fn test_ipv4_sanitize_clears_when_ip_disabled() {
    let mut ipv4 = InterfaceIpv4 {
        enabled: Some(false),
        dhcp: Some(true),
        dhcp_state: None,
        addresses: None,
        auto_gateway: Some(false),
    };
    ipv4.sanitize(None).unwrap();
    assert_eq!(ipv4.auto_gateway, None);
}
