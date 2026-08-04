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

#[test]
fn test_yaml_round_trip_bond_with_ports() {
    let (state, reparsed) = round_trip(
        r#"---
        interfaces:
          - name: bond0
            type: bond
            bond:
              mode: active-backup
              options:
                miimon: 100
          - name: eth1
            type: ethernet
            controller: bond0
          - name: eth2
            type: ethernet
            controller: bond0
        "#,
    );
    assert_eq!(state, reparsed);
}

#[test]
fn test_yaml_round_trip_linux_bridge() {
    let (state, reparsed) = round_trip(
        r#"---
        interfaces:
          - name: br0
            type: linux-bridge
            bridge:
              options:
                stp:
                  enabled: true
              port:
                - name: eth1
        "#,
    );
    assert_eq!(state, reparsed);
}

#[test]
fn test_yaml_round_trip_vlan_and_vxlan() {
    let (state, reparsed) = round_trip(
        r#"---
        interfaces:
          - name: eth1.100
            type: vlan
            vlan:
              base-iface: eth1
              id: 100
          - name: vxlan0
            type: vxlan
            vxlan:
              base-iface: eth1.100
              id: 100
        "#,
    );
    assert_eq!(state, reparsed);
}

#[test]
fn test_yaml_round_trip_wireguard() {
    let (state, reparsed) = round_trip(
        r#"---
        interfaces:
          - name: wg0
            type: wireguard
            wireguard:
              listen-port: 51820
              private-key: aGFuZCBvdmVyIHRoZSBrZXk=
              peers:
                - public-key: cGVlciBwdWJsaWMga2V5
                  preshared-key: cHJlLXNoYXJlZA==
                  endpoint: 192.0.2.2:51820
                  persistent-keepalive: 25
                  allowed-ips:
                    - ip: 192.0.2.0
                      prefix-length: 24
        "#,
    );
    assert_eq!(state, reparsed);
}

#[test]
fn test_yaml_round_trip_routes() {
    let (state, reparsed) = round_trip(
        r#"---
        routes:
          config:
            - destination: 0.0.0.0/0
              next-hop-address: 192.0.2.1
              next-hop-interface: eth1
              metric: 100
            - destination: 198.51.100.0/24
              next-hop-interface: eth2
              state: absent
          running:
            - destination: 203.0.113.0/24
              next-hop-address: 192.0.2.9
              next-hop-interface: eth1
        "#,
    );
    assert_eq!(state, reparsed);
}
