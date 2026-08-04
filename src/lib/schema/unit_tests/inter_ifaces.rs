// SPDX-License-Identifier: Apache-2.0

use crate::{
    BaseInterface, InterfaceIdentifier, InterfaceIpv4, InterfaceIpv6,
    InterfaceState, InterfaceType, Interfaces, MergedInterfaces,
    MergedNetworkState, NetworkState, NipartInterface,
};

/// Test basic MAC address matching with MAC provided.
#[test]
fn test_resolve_mac_identifier_basic_with_mac() {
    let desired: Interfaces = serde_yaml::from_str(
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

    let merged = MergedInterfaces::new(desired, current, None).unwrap();

    // Interface should be keyed by kernel name
    assert!(merged.kernel_ifaces.contains_key("eth0"));
    let merged_iface = merged.kernel_ifaces.get("eth0").unwrap();
    let for_apply = merged_iface.for_apply.as_ref().unwrap();
    assert_eq!(for_apply.base_iface().kernel_iface_name.as_str(), "eth0");
}

/// Test MAC address matching with `permanent-mac-address`.
#[test]
fn test_resolve_mac_identifier_perm_mac() {
    let desired: Interfaces = serde_yaml::from_str(
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

    let merged = MergedInterfaces::new(desired, current, None).unwrap();

    let merged_iface = merged.kernel_ifaces.get("eth0").unwrap();
    let for_apply = merged_iface.for_apply.as_ref().unwrap();
    assert_eq!(for_apply.base_iface().kernel_iface_name.as_str(), "eth0");
}

/// Test error when MAC does not match any current interface.
#[test]
fn test_resolve_mac_identifier_no_match() {
    let desired: Interfaces = serde_yaml::from_str(
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

    let result = MergedInterfaces::new(desired, current, None);
    assert!(result.is_err());
}

/// Test re-resolution when profile_name already set and NIC name changed.
#[test]
fn test_resolve_mac_identifier_re_resolve() {
    let desired: Interfaces = serde_yaml::from_str(
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

    let merged = MergedInterfaces::new(desired, current, None).unwrap();

    // Should be keyed by new kernel name
    assert!(merged.kernel_ifaces.contains_key("enp0s3"));
    let merged_iface = merged.kernel_ifaces.get("enp0s3").unwrap();
    let for_apply = merged_iface.for_apply.as_ref().unwrap();
    assert_eq!(for_apply.base_iface().kernel_iface_name.as_str(), "enp0s3");
}

/// Test that absent interfaces with MAC identifier still merge correctly.
#[test]
fn test_resolve_mac_identifier_absent_skipped() {
    let desired: Interfaces = serde_yaml::from_str(
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

    let merged = MergedInterfaces::new(desired, current, None).unwrap();

    // Absent interface should still be matched by MAC and keyed by
    // kernel name
    assert!(merged.kernel_ifaces.contains_key("eth0"));
    let merged_iface = merged.kernel_ifaces.get("eth0").unwrap();
    // Non-virtual interface should have state: down in for_apply
    assert_eq!(
        merged_iface.for_apply.as_ref().unwrap().base_iface().state,
        InterfaceState::Down
    );
    // for_save should preserve the absent intent
    assert_eq!(
        merged_iface.for_save.as_ref().unwrap().base_iface().state,
        InterfaceState::Absent
    );
}

/// Test resolving interface with MAC identifier matches by MAC
/// regardless of interface type in desired state.
#[test]
fn test_resolve_mac_identifier_unknown_type() {
    let desired: Interfaces = serde_yaml::from_str(
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

    let merged = MergedInterfaces::new(desired, current, None).unwrap();

    let merged_iface = merged.kernel_ifaces.get("eth0").unwrap();
    // Merged type should be Ethernet
    assert_eq!(merged_iface.merged.iface_type(), &InterfaceType::Ethernet);
}

/// Test error when mac_address is not provided.
#[test]
fn test_resolve_mac_identifier_missing_mac() {
    let desired: Interfaces = serde_yaml::from_str(
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

    let result = MergedInterfaces::new(desired, current, None);
    assert!(result.is_err());
}

/// Test that already-resolved interface (name matches kernel name) works.
#[test]
fn test_resolve_mac_identifier_already_resolved() {
    let desired: Interfaces = serde_yaml::from_str(
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

    let merged = MergedInterfaces::new(desired, current, None).unwrap();

    // Should still have eth0
    assert!(merged.kernel_ifaces.contains_key("eth0"));
    let merged_iface = merged.kernel_ifaces.get("eth0").unwrap();
    let for_apply = merged_iface.for_apply.as_ref().unwrap();
    assert_eq!(for_apply.base_iface().kernel_iface_name.as_str(), "eth0");
}

/// Test case-insensitive MAC address matching.
#[test]
fn test_resolve_mac_identifier_case_insensitive() {
    let desired: Interfaces = serde_yaml::from_str(
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

    let merged = MergedInterfaces::new(desired, current, None).unwrap();

    assert!(merged.kernel_ifaces.contains_key("eth0"));
    let merged_iface = merged.kernel_ifaces.get("eth0").unwrap();
    let for_apply = merged_iface.for_apply.as_ref().unwrap();
    assert_eq!(for_apply.base_iface().kernel_iface_name.as_str(), "eth0");
}

/// Test resolving multiple interfaces with MAC identifiers.
#[test]
fn test_resolve_mac_identifier_multiple_ifaces() {
    let desired: Interfaces = serde_yaml::from_str(
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

    let merged = MergedInterfaces::new(desired, current, None).unwrap();

    assert!(merged.kernel_ifaces.contains_key("eth0"));
    assert!(merged.kernel_ifaces.contains_key("eth1"));
    let merged1 = merged.kernel_ifaces.get("eth0").unwrap();
    let for_apply1 = merged1.for_apply.as_ref().unwrap();
    assert_eq!(for_apply1.base_iface().kernel_iface_name.as_str(), "eth0");
    let merged2 = merged.kernel_ifaces.get("eth1").unwrap();
    let for_apply2 = merged2.for_apply.as_ref().unwrap();
    assert_eq!(for_apply2.base_iface().kernel_iface_name.as_str(), "eth1");
}

/// Test that permanent_mac_address is preferred over mac_address when
/// matching.
#[test]
fn test_resolve_mac_identifier_perm_mac_preferred() {
    let desired: Interfaces = serde_yaml::from_str(
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

    let merged = MergedInterfaces::new(desired, current, None).unwrap();

    // Should match eth0 (permanent_mac_address match)
    assert!(merged.kernel_ifaces.contains_key("eth0"));
    let merged_iface = merged.kernel_ifaces.get("eth0").unwrap();
    assert!(merged_iface.is_desired());
}

/// Test that matching against multiple NICs with the same MAC picks the
/// first match.
#[test]
fn test_resolve_mac_identifier_duplicate_mac_across_nics() {
    let desired: Interfaces = serde_yaml::from_str(
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

    // Should succeed, picking the first MAC match
    let merged = MergedInterfaces::new(desired, current, None).unwrap();
    // One of eth0/eth1 should be the desired matched interface
    let desired_count = merged
        .kernel_ifaces
        .values()
        .filter(|i| i.is_desired())
        .count();
    assert_eq!(desired_count, 1);
}

/// Test re-resolution when NIC renamed (eth0 -> eth1).
#[test]
fn test_resolve_mac_identifier_re_resolve_type_change() {
    let desired: Interfaces = serde_yaml::from_str(
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

    let merged = MergedInterfaces::new(desired, current, None).unwrap();

    assert!(merged.kernel_ifaces.contains_key("eth1"));
    let merged_iface = merged.kernel_ifaces.get("eth1").unwrap();
    let for_apply = merged_iface.for_apply.as_ref().unwrap();
    assert_eq!(for_apply.base_iface().kernel_iface_name.as_str(), "eth1");
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
        MergedNetworkState::new(desired, current, None, Default::default())
            .unwrap();
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

/// Test that sanitize converts MAC addresses to uppercase.
#[test]
fn test_sanitize_mac_address_to_uppercase() {
    let mut base =
        BaseInterface::new("eth0".to_string(), InterfaceType::Ethernet);
    base.identifier = Some(InterfaceIdentifier::MacAddress);
    base.mac_address = Some("00:23:45:67:89:1b".to_string());
    base.permanent_mac_address = Some("aa:bb:cc:dd:ee:ff".to_string());

    let mut for_save = base.clone();
    let mut for_apply = base.clone();
    let mut for_verify = base.clone();
    let mut merged = base.clone();

    base.sanitize(
        None,
        &mut for_save,
        &mut for_apply,
        &mut for_verify,
        &mut merged,
    )
    .unwrap();

    assert_eq!(for_apply.mac_address.as_deref(), Some("00:23:45:67:89:1B"));
    assert_eq!(for_verify.mac_address.as_deref(), Some("00:23:45:67:89:1B"));
    assert_eq!(for_save.mac_address.as_deref(), Some("00:23:45:67:89:1B"));
    assert_eq!(merged.mac_address.as_deref(), Some("00:23:45:67:89:1B"));
    // permanent_mac_address is query-only, only merged should hold it
    assert_eq!(for_apply.permanent_mac_address.as_deref(), None);
    assert_eq!(for_save.permanent_mac_address.as_deref(), None);
    assert_eq!(for_verify.permanent_mac_address.as_deref(), None);
    assert_eq!(
        merged.permanent_mac_address.as_deref(),
        Some("AA:BB:CC:DD:EE:FF")
    );
}

/// Test that sanitize does NOT override kernel_iface_name for MacAddress
/// identifier.
#[test]
fn test_sanitize_does_not_override_mac_kernel_iface_name() {
    let base = BaseInterface::new("wan0".to_string(), InterfaceType::Ethernet);
    let mut for_save = base.clone();
    let mut for_apply = base.clone();
    let mut for_verify = base.clone();
    let mut merged = base.clone();
    for_apply.identifier = Some(InterfaceIdentifier::MacAddress);
    for_apply.kernel_iface_name = "eth0".to_string();
    base.sanitize(
        None,
        &mut for_save,
        &mut for_apply,
        &mut for_verify,
        &mut merged,
    )
    .unwrap();
    // kernel_iface_name should be preserved (not overwritten by sanitize)
    assert_eq!(for_apply.kernel_iface_name.as_str(), "eth0");
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
        MergedNetworkState::new(desired, current, None, Default::default())
            .unwrap();

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
        MergedNetworkState::new(desired, current, None, Default::default())
            .unwrap();

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

    let result =
        MergedNetworkState::new(desired, current, None, Default::default());
    assert!(result.is_err());
    assert!(result.unwrap_err().msg.contains("marked as absent"));
}

/// Test that resolve_route_next_hop_iface with duplicate profile_name
/// resolves to one of the matches via MergedInterfaces::new().
#[test]
fn test_route_next_hop_iface_duplicate_logical_name() {
    let desired: Interfaces = serde_yaml::from_str(
        r#"---
        - name: eth0
          type: ethernet
          state: up
          profile-name: cunet
        - name: eth1
          type: ethernet
          state: up
          profile-name: cunet
        "#,
    )
    .unwrap();

    let current: Interfaces = serde_yaml::from_str(
        r#"---
        - name: eth0
          type: ethernet
        - name: eth1
          type: ethernet
        "#,
    )
    .unwrap();

    let merged_ifaces = MergedInterfaces::new(desired, current, None).unwrap();

    // IfaceSearch stores last inserted profile mapping
    let result = merged_ifaces.resolve_route_next_hop_iface("cunet");
    assert!(result.is_some());
}

/// Test that resolve_route_next_hop_iface returns None
/// when no match is found (neither kernel name nor profile_name).
#[test]
fn test_route_next_hop_iface_no_match_returns_none() {
    let merged_ifaces = MergedInterfaces::default();

    let result = merged_ifaces.resolve_route_next_hop_iface("nonexistent");
    assert!(result.is_none());
}

/// Test that resolve_route_next_hop_iface returns kernel name when
/// a single profile_name match is found via MergedInterfaces::new().
#[test]
fn test_route_next_hop_iface_single_profile_match() {
    let desired: Interfaces = serde_yaml::from_str(
        r#"---
        - name: eth0
          type: ethernet
          state: up
          profile-name: cunet
        "#,
    )
    .unwrap();

    let current: Interfaces = serde_yaml::from_str(
        r#"---
        - name: eth0
          type: ethernet
        "#,
    )
    .unwrap();

    let merged_ifaces = MergedInterfaces::new(desired, current, None).unwrap();

    let result = merged_ifaces.resolve_route_next_hop_iface("cunet");
    assert_eq!(result, Some("eth0".to_string()));
}

/// Test that route with next-hop-interface pointing to a non-existent
/// interface raises error.
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

    let result =
        MergedNetworkState::new(desired, current, None, Default::default());
    assert!(result.is_err());
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

/// Test that bond port names are resolved from profile names to kernel
/// interface names when ports use MAC address identifier.
#[test]
fn test_bond_resolve_port_ref_by_mac_identifier() {
    let desired: Interfaces = serde_yaml::from_str(
        r#"---
        - name: bond1
          type: bond
          state: up
          bond:
            mode: active-backup
            ports:
            - name: port1
            - name: port2
        - name: port1
          type: ethernet
          mac-address: 00:23:45:67:89:1a
          identifier: mac-address
        - name: port2
          type: ethernet
          mac-address: 00:23:45:67:89:1b
          identifier: mac-address"#,
    )
    .unwrap();

    let current: Interfaces = serde_yaml::from_str(
        r#"---
        - name: eth0
          type: ethernet
          mac-address: 00:23:45:67:89:1a
        - name: eth1
          type: ethernet
          mac-address: 00:23:45:67:89:1b"#,
    )
    .unwrap();

    let merged = MergedInterfaces::new(desired, current, None).unwrap();

    let bond = merged.kernel_ifaces.get("bond1").unwrap();

    assert_eq!(
        bond.for_apply.as_ref().unwrap().ports().unwrap(),
        vec!["eth0", "eth1"]
    );
    assert_eq!(
        bond.for_verify.as_ref().unwrap().ports().unwrap(),
        vec!["eth0", "eth1"]
    );
    assert_eq!(
        bond.for_save.as_ref().unwrap().ports().unwrap(),
        vec!["port1", "port2"]
    );
    assert_eq!(bond.merged.ports().unwrap(), vec!["eth0", "eth1"]);
}

/// Test that linux bridge port names are resolved from profile names to
/// kernel interface names when ports use MAC address identifier.
#[test]
fn test_linux_bridge_resolve_port_ref_by_mac_identifier() {
    let desired: Interfaces = serde_yaml::from_str(
        r#"---
        - name: br0
          type: linux-bridge
          state: up
          bridge:
            port:
            - name: port1
        - name: port1
          type: ethernet
          mac-address: 00:23:45:67:89:1a
          identifier: mac-address"#,
    )
    .unwrap();

    let current: Interfaces = serde_yaml::from_str(
        r#"---
        - name: eth0
          type: ethernet
          mac-address: 00:23:45:67:89:1a"#,
    )
    .unwrap();

    let merged = MergedInterfaces::new(desired, current, None).unwrap();

    let br = merged.kernel_ifaces.get("br0").unwrap();

    assert_eq!(
        br.for_apply.as_ref().unwrap().ports().unwrap(),
        vec!["eth0"]
    );
    assert_eq!(
        br.for_verify.as_ref().unwrap().ports().unwrap(),
        vec!["eth0"]
    );
    assert_eq!(
        br.for_save.as_ref().unwrap().ports().unwrap(),
        vec!["port1"]
    );
    assert_eq!(br.merged.ports().unwrap(), vec!["eth0"]);
}

/// Test that OVS bridge port names are resolved from profile names to
/// kernel interface names when ports use MAC address identifier.
#[test]
fn test_ovs_bridge_resolve_port_ref_by_mac_identifier() {
    let desired: Interfaces = serde_yaml::from_str(
        r#"---
        - name: ovs-br0
          type: ovs-bridge
          state: up
          bridge:
            ports:
            - name: port1
        - name: port1
          type: ethernet
          mac-address: 00:23:45:67:89:1a
          identifier: mac-address"#,
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

    let merged = MergedInterfaces::new(desired, current, None).unwrap();

    let br = merged
        .user_ifaces
        .get(&("ovs-br0".to_string(), crate::InterfaceType::OvsBridge))
        .unwrap();

    assert_eq!(
        br.for_apply.as_ref().unwrap().ports().unwrap(),
        vec!["eth0"]
    );
    assert_eq!(
        br.for_verify.as_ref().unwrap().ports().unwrap(),
        vec!["eth0"]
    );
    assert_eq!(
        br.for_save.as_ref().unwrap().ports().unwrap(),
        vec!["port1"]
    );
    assert_eq!(br.merged.ports().unwrap(), vec!["eth0"]);
}

/// Test gen_state_for_apply() and gen_state_for_save() with bond ports
/// resolved by MAC identifier matching uppercase MACs in current state.
#[test]
fn test_bond_gen_state_for_apply_and_save() {
    let desired: Interfaces = serde_yaml::from_str(
        r#"---
        - name: port1
          type: ethernet
          identifier: mac-address
          mac-address: 00:23:45:67:89:1a
        - name: port2
          type: ethernet
          identifier: mac-address
          mac-address: 00:23:45:67:89:1b
        - name: bond0
          kernel-iface-name: bond0
          type: bond
          state: up
          bond:
            mode: balance-rr
            ports:
            - name: port1
            - name: port2"#,
    )
    .unwrap();

    let current: Interfaces = serde_yaml::from_str(
        r#"---
        - name: eth1
          kernel-iface-name: eth1
          type: ethernet
          state: down
          mac-address: 00:23:45:67:89:1A
        - name: eth2
          kernel-iface-name: eth2
          type: ethernet
          state: down
          mac-address: 00:23:45:67:89:1B"#,
    )
    .unwrap();

    let merged = MergedInterfaces::new(desired, current, None).unwrap();

    let apply_state = merged.gen_state_for_apply();
    let save_state = merged.gen_state_for_save();

    let eth1_apply = apply_state.kernel_ifaces.get("eth1").unwrap();
    let eth1_saved = save_state.kernel_ifaces.get("port1").unwrap();

    let eth2_apply = apply_state.kernel_ifaces.get("eth2").unwrap();
    let eth2_saved = save_state.kernel_ifaces.get("port2").unwrap();
    assert_eq!(eth1_apply.name(), "eth1");
    assert_eq!(eth1_saved.name(), "port1");
    assert_eq!(
        eth1_apply.base_iface().profile_name.as_deref(),
        Some("port1")
    );
    assert_eq!(
        eth1_saved.base_iface().profile_name.as_deref(),
        Some("port1")
    );
    assert_eq!(eth2_apply.name(), "eth2");
    assert_eq!(eth2_saved.name(), "port2");
    assert_eq!(
        eth2_apply.base_iface().profile_name.as_deref(),
        Some("port2")
    );
    assert_eq!(
        eth2_saved.base_iface().profile_name.as_deref(),
        Some("port2")
    );
}

/// Test that BaseInterface::sanitize preserves for_save IP config when
/// for_apply has no IP changes (the diff omitted ipv4/ipv6 because the
/// current kernel state already matches).
#[test]
fn test_sanitize_preserves_ip_when_for_apply_has_no_ip() {
    let mut base =
        BaseInterface::new("eth0".to_string(), InterfaceType::Ethernet);
    base.ipv4 = Some(InterfaceIpv4 {
        enabled: Some(true),
        dhcp: Some(true),
        auto_gateway: Some(false),
        ..Default::default()
    });
    base.ipv6 = Some(InterfaceIpv6 {
        enabled: Some(true),
        dhcp: Some(true),
        autoconf: Some(false),
        ..Default::default()
    });

    // for_save has the full desired IP config
    let mut for_save = base.clone();
    let mut for_verify = base.clone();
    let mut merged = base.clone();

    // for_apply has NO ipv4/ipv6 (diff produced no IP changes)
    let mut for_apply = base.clone();
    for_apply.ipv4 = None;
    for_apply.ipv6 = None;

    base.sanitize(
        None,
        &mut for_save,
        &mut for_apply,
        &mut for_verify,
        &mut merged,
    )
    .unwrap();

    // for_save should still have the original IP config
    let ipv4 = for_save.ipv4.as_ref().expect("ipv4 should be preserved");
    assert_eq!(ipv4.enabled, Some(true));
    assert_eq!(ipv4.dhcp, Some(true));
    assert_eq!(ipv4.auto_gateway, Some(false));

    let ipv6 = for_save.ipv6.as_ref().expect("ipv6 should be preserved");
    assert_eq!(ipv6.enabled, Some(true));
    assert_eq!(ipv6.dhcp, Some(true));
    assert_eq!(ipv6.autoconf, Some(false));
}

/// Test that BaseInterface::sanitize still copies sanitized IP from for_apply
/// to for_save when for_apply has IP changes.
#[test]
fn test_sanitize_copies_ip_when_for_apply_has_ip_changes() {
    let mut base =
        BaseInterface::new("eth0".to_string(), InterfaceType::Ethernet);
    base.ipv4 = Some(InterfaceIpv4 {
        enabled: Some(true),
        dhcp: Some(true),
        auto_gateway: Some(false),
        ..Default::default()
    });

    // for_apply has ipv4 with a different config (e.g. dhcp changed to false)
    let mut for_save = base.clone();
    let mut for_verify = base.clone();
    let mut merged = base.clone();
    let mut for_apply = base.clone();
    for_apply.ipv4 = Some(InterfaceIpv4 {
        enabled: Some(true),
        dhcp: Some(false),
        ..Default::default()
    });

    base.sanitize(
        None,
        &mut for_save,
        &mut for_apply,
        &mut for_verify,
        &mut merged,
    )
    .unwrap();

    // for_save should have the sanitized ipv4 from for_apply
    // dhcp: false → after sanitize, auto_gateway is set to None
    let ipv4 = for_save.ipv4.as_ref().expect("ipv4 should be present");
    assert_eq!(ipv4.enabled, Some(true));
    assert_eq!(ipv4.dhcp, Some(false));
    // auto_gateway should be None because dhcp != Some(true)
    assert_eq!(ipv4.auto_gateway, None);
}
