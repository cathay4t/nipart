// SPDX-License-Identifier: Apache-2.0

use super::*;

fn rule_from_yaml(yaml: &str) -> RouteRuleEntry {
    rmsd_yaml::from_str(yaml).unwrap()
}

#[test]
fn test_route_rule_sanitize_single_host_adds_prefix_length() {
    let mut rule = rule_from_yaml(
        r#"
            ip-from: 203.0.113.1
            ip-to: 192.0.2.0/24
            route-table: 254
            "#,
    );
    rule.sanitize().unwrap();
    assert_eq!(rule.ip_from.as_deref(), Some("203.0.113.1/32"));
    assert_eq!(rule.family, Some(AddressFamily::Ipv4));
}

#[test]
fn test_route_rule_absent_wildcard_matches_table() {
    let absent = rule_from_yaml(
        r#"
            state: absent
            route-table: 500
            "#,
    );
    let current = rule_from_yaml(
        r#"
            ip-from: 203.0.113.0/24
            route-table: 500
            priority: 1000
            "#,
    );
    assert!(absent.is_match(&current));
    assert!(!absent.is_match(&rule_from_yaml(
        r#"
            route-table: 501
            "#,
    )));
}

#[test]
fn test_route_rule_fwmask_without_fwmark_rejected() {
    let mut rule = rule_from_yaml(
        r#"
            ip-from: 203.0.113.0/24
            route-table: 500
            fwmask: 0x10
            "#,
    );
    assert!(rule.sanitize().is_err());
}

#[test]
fn test_route_rule_default_route_table() {
    let mut rule = rule_from_yaml(
        r#"
            ip-from: 203.0.113.1/32
            "#,
    );
    rule.sanitize().unwrap();
    assert_eq!(rule.table_id, Some(254));
}

#[test]
fn test_route_rule_merge_is_partial() {
    let old: RouteRules = rmsd_yaml::from_str(
        r#"
            config:
              - ip-from: 203.0.113.0/24
                route-table: 500
              - ip-from: 198.51.100.0/24
                route-table: 500
            "#,
    )
    .unwrap();
    let new: RouteRules = rmsd_yaml::from_str(
        r#"
            config:
              - state: absent
                route-table: 500
                ip-from: 198.51.100.0/24
              - ip-from: 192.0.2.0/24
                route-table: 600
            "#,
    )
    .unwrap();

    let merged = old.merge(&new).unwrap();
    let rules = merged.config.unwrap();
    assert_eq!(rules.len(), 2);
    assert!(
        rules
            .iter()
            .any(|r| r.ip_from.as_deref() == Some("203.0.113.0/24"))
    );
    assert!(
        rules
            .iter()
            .any(|r| r.ip_from.as_deref() == Some("192.0.2.0/24"))
    );
}
