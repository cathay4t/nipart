// SPDX-License-Identifier: Apache-2.0

use nipart::NetworkState;

use super::append_saved_only_route_rules;

#[test]
fn test_saved_route_rule_without_priority_not_duplicated() {
    let mut net_state: NetworkState = rmsd_yaml::from_str(
        r#"---
            route-rules:
              config:
                - ip-from: 198.51.100.1/32
                  route-table: 254
                  priority: 30000
            "#,
    )
    .unwrap();
    let saved_state: NetworkState = rmsd_yaml::from_str(
        r#"---
            route-rules:
              config:
                - ip-from: 198.51.100.1
            "#,
    )
    .unwrap();

    append_saved_only_route_rules(&mut net_state, &saved_state);

    let rules = net_state.route_rules.config.unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].priority, Some(30000));
}

#[test]
fn test_saved_default_table_rule_not_matched_by_action_rule() {
    let mut net_state: NetworkState = rmsd_yaml::from_str(
        r#"---
            route-rules:
              config:
                - ip-from: 198.51.100.1/32
                  action: blackhole
                  priority: 30000
            "#,
    )
    .unwrap();
    let saved_state: NetworkState = rmsd_yaml::from_str(
        r#"---
            route-rules:
              config:
                - ip-from: 198.51.100.1
            "#,
    )
    .unwrap();

    append_saved_only_route_rules(&mut net_state, &saved_state);

    let rules = net_state.route_rules.config.unwrap();
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[1].ip_from.as_deref(), Some("198.51.100.1"));
    assert_eq!(rules[1].table_id, Some(254));
}
