// SPDX-License-Identifier: Apache-2.0

use crate::{
    AddressFamily, Interfaces, MergedInterfaces, MergedRouteRules,
    NetworkState, RouteRuleEntry, RouteRuleState, RouteRules,
};

#[test]
fn test_route_rules_deserialize_and_round_trip() {
    let state = NetworkState::new_from_yaml(
        r#"---
        route-rules:
          config:
            - family: ipv4
              ip-from: 198.51.100.0/24
              ip-to: 192.0.2.0/24
              priority: 100
              route-table: 500
              fwmark: 0x30
              fwmask: 0x10
              action: blackhole
              iif: eth1
              suppress-prefix-length: 24
        "#,
    )
    .unwrap();

    let rules = state.route_rules.config.as_ref().unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].family, Some(AddressFamily::Ipv4));
    assert_eq!(rules[0].ip_from.as_deref(), Some("198.51.100.0/24"));
    assert_eq!(rules[0].fwmark, Some(0x30));
    assert_eq!(rules[0].suppress_prefix_length, Some(24));

    let yaml = rmsd_yaml::to_string(&state).unwrap();
    let reparsed = NetworkState::new_from_yaml(&yaml).unwrap();
    assert_eq!(state, reparsed);
}

#[test]
fn test_route_rule_absent_marks_current_rule_for_removal() {
    let desired: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - state: absent
            route-table: 500
        "#,
    )
    .unwrap();
    let current: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - ip-from: 198.51.100.0/24
            route-table: 500
            priority: 100
          - ip-from: 203.0.113.0/24
            route-table: 600
            priority: 200
        "#,
    )
    .unwrap();

    let merged = MergedRouteRules::new(
        desired,
        current,
        None,
        &MergedInterfaces::default(),
    )
    .unwrap();
    assert_eq!(merged.changed_rules.len(), 1);
    let absent_rule = &merged.changed_rules[0];
    assert!(absent_rule.is_absent());
    assert_eq!(absent_rule.table_id, Some(500));
    assert_eq!(absent_rule.ip_from.as_deref(), Some("198.51.100.0/24"));
    assert!(merged.gen_state_for_save().config.is_none());
}

#[test]
fn test_existing_route_rule_is_not_changed() {
    let desired: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - ip-from: 198.51.100.0/24
            route-table: 500
        "#,
    )
    .unwrap();
    let current: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - ip-from: 198.51.100.0/24
            route-table: 500
            priority: 30001
        "#,
    )
    .unwrap();

    let merged = MergedRouteRules::new(
        desired,
        current,
        None,
        &MergedInterfaces::default(),
    )
    .unwrap();
    assert!(merged.changed_rules.is_empty());
    assert!(!merged.is_changed());
}

#[test]
fn test_new_route_rule_without_priority_gets_auto_priority() {
    let desired: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - ip-from: 203.0.113.0/24
            route-table: 500
        "#,
    )
    .unwrap();
    let current: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - ip-from: 198.51.100.0/24
            route-table: 500
            priority: 30010
        "#,
    )
    .unwrap();

    let merged = MergedRouteRules::new(
        desired,
        current,
        None,
        &MergedInterfaces::default(),
    )
    .unwrap();
    assert_eq!(merged.changed_rules.len(), 1);
    let new_rule = &merged.changed_rules[0];
    assert!(!new_rule.is_absent());
    assert_eq!(new_rule.priority, Some(30011));
}

#[test]
fn test_route_rule_absent_wildcard_matches_saved_config() {
    let desired: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - state: absent
            route-table: 500
        "#,
    )
    .unwrap();
    let current = RouteRules::default();
    let saved: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - ip-from: 198.51.100.0/24
            route-table: 500
        "#,
    )
    .unwrap();

    let merged = MergedRouteRules::new(
        desired,
        current,
        Some(saved),
        &MergedInterfaces::default(),
    )
    .unwrap();
    assert!(merged.gen_state_for_save().config.is_none());
}

#[test]
fn test_route_rule_for_save_is_sanitized() {
    let desired: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - ip-from: 203.0.113.1
        "#,
    )
    .unwrap();

    let merged = MergedRouteRules::new(
        desired,
        RouteRules::default(),
        None,
        &MergedInterfaces::default(),
    )
    .unwrap();
    let saved_rules = merged.gen_state_for_save().config.unwrap();

    assert_eq!(saved_rules.len(), 1);
    assert_eq!(saved_rules[0].ip_from.as_deref(), Some("203.0.113.1/32"));
    assert_eq!(saved_rules[0].family, Some(AddressFamily::Ipv4));
    assert_eq!(
        saved_rules[0].table_id,
        Some(RouteRuleEntry::DEFAULT_ROUTE_TABLE_ID)
    );
    assert_eq!(saved_rules[0].priority, None);
}

#[test]
fn test_route_rule_absent_removes_noncanonical_saved_rule() {
    let desired: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - state: absent
            ip-from: 203.0.113.1/32
            route-table: 500
        "#,
    )
    .unwrap();
    let current = RouteRules::default();
    let saved: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - ip-from: 203.0.113.1
            route-table: 500
        "#,
    )
    .unwrap();

    let merged = MergedRouteRules::new(
        desired,
        current,
        Some(saved),
        &MergedInterfaces::default(),
    )
    .unwrap();
    assert!(merged.gen_state_for_save().config.is_none());
}

#[test]
fn test_route_rule_state_absent_is_default() {
    assert_eq!(RouteRuleState::default(), RouteRuleState::Absent);
    let rule = RouteRuleEntry::default();
    assert!(!rule.is_absent());
    let absent = RouteRuleEntry {
        state: Some(RouteRuleState::Absent),
        ..Default::default()
    };
    assert!(absent.is_absent());
}

#[test]
fn test_reapply_without_priority_inherits_saved_priority() {
    let desired: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - ip-from: 198.51.100.0/24
            route-table: 500
        "#,
    )
    .unwrap();
    let current: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - ip-from: 198.51.100.0/24
            route-table: 500
            priority: 40000
        "#,
    )
    .unwrap();
    let saved = current.clone();

    let merged = MergedRouteRules::new(
        desired,
        current,
        Some(saved),
        &MergedInterfaces::default(),
    )
    .unwrap();

    assert!(merged.changed_rules.is_empty());
    let saved_rules = merged.gen_state_for_save().config.unwrap();
    assert_eq!(saved_rules.len(), 1);
    assert_eq!(saved_rules[0].priority, Some(40000));
}

#[test]
fn test_reapply_without_priority_self_heals_saved_priority() {
    let desired: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - ip-from: 198.51.100.0/24
            route-table: 500
        "#,
    )
    .unwrap();
    let saved: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - ip-from: 198.51.100.0/24
            route-table: 500
            priority: 40000
        "#,
    )
    .unwrap();

    let merged = MergedRouteRules::new(
        desired,
        RouteRules::default(),
        Some(saved),
        &MergedInterfaces::default(),
    )
    .unwrap();

    assert_eq!(merged.changed_rules.len(), 1);
    assert_eq!(merged.changed_rules[0].priority, Some(40000));
    let saved_rules = merged.gen_state_for_save().config.unwrap();
    assert_eq!(saved_rules.len(), 1);
    assert_eq!(saved_rules[0].priority, Some(40000));
}

#[test]
fn test_deleted_iface_removes_rule_from_kernel_and_saved() {
    let desired_ifaces: Interfaces = rmsd_yaml::from_str(
        r#"---
        - name: dummy0
          type: dummy
          state: absent
        "#,
    )
    .unwrap();
    let current_ifaces: Interfaces = rmsd_yaml::from_str(
        r#"---
        - name: dummy0
          type: dummy
          state: up
          ipv4:
            enabled: false
          ipv6:
            enabled: false
        "#,
    )
    .unwrap();
    let merged_ifaces =
        MergedInterfaces::new(desired_ifaces, current_ifaces, None).unwrap();

    let current: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - family: ipv4
            iif: dummy0
            route-table: 500
            priority: 100
        "#,
    )
    .unwrap();
    let saved = current.clone();

    let merged = MergedRouteRules::new(
        RouteRules::default(),
        current,
        Some(saved),
        &merged_ifaces,
    )
    .unwrap();

    assert_eq!(merged.changed_rules.len(), 1);
    assert!(merged.changed_rules[0].is_absent());
    assert!(merged.gen_state_for_save().config.is_none());
}

#[test]
fn test_rule_iif_profile_name_resolved_for_apply() {
    let desired_ifaces: Interfaces = rmsd_yaml::from_str(
        r#"---
        - name: eth1
          type: ethernet
          state: up
          profile-name: wan0
          ipv4:
            enabled: false
          ipv6:
            enabled: false
        "#,
    )
    .unwrap();
    let current_ifaces: Interfaces = rmsd_yaml::from_str(
        r#"---
        - name: eth1
          type: ethernet
          state: up
        "#,
    )
    .unwrap();
    let merged_ifaces =
        MergedInterfaces::new(desired_ifaces, current_ifaces, None).unwrap();

    let desired: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - family: ipv4
            iif: wan0
            route-table: 500
        "#,
    )
    .unwrap();

    let merged = MergedRouteRules::new(
        desired,
        RouteRules::default(),
        None,
        &merged_ifaces,
    )
    .unwrap();

    assert_eq!(merged.changed_rules.len(), 1);
    assert_eq!(merged.changed_rules[0].iif.as_deref(), Some("eth1"));

    // The profile name is persisted so it still resolves when the kernel
    // interface name changes across reboots.
    let saved_rules = merged.gen_state_for_save().config.unwrap();
    assert_eq!(saved_rules.len(), 1);
    assert_eq!(saved_rules[0].iif.as_deref(), Some("wan0"));

    // Verification uses the kernel interface name.
    let current: RouteRules = rmsd_yaml::from_str(
        r#"---
        config:
          - family: ipv4
            iif: eth1
            route-table: 500
            priority: 30000
        "#,
    )
    .unwrap();
    merged.verify(&current).unwrap();
}
