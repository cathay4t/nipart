// SPDX-License-Identifier: Apache-2.0

use crate::{NetworkState, NipartWaitOnlineCondition};

fn round_trip(yaml_str: &str) -> (NetworkState, NetworkState) {
    let state = NetworkState::new_from_yaml(yaml_str).unwrap();
    let serialized = serde_yaml::to_string(&state).unwrap();
    let reparsed = NetworkState::new_from_yaml(&serialized).unwrap();
    (state, reparsed)
}

#[test]
fn test_yaml_round_trip_ethernet_with_ip() {
    let (state, reparsed) = round_trip(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            mtu: 9000
            mac-address: DE:AD:BE:EF:00:01
            ipv4:
              enabled: true
              dhcp: false
              address:
                - ip: 192.0.2.251
                  prefix-length: 24
            ipv6:
              enabled: true
              dhcp: true
              autoconf: true
        "#,
    );
    assert_eq!(state, reparsed);
}

