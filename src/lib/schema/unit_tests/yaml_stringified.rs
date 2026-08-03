// SPDX-License-Identifier: Apache-2.0

use crate::{BondMode, Interface, NetworkState, NipartInterface};

#[test]
fn test_yaml_stringified_ip_prefix_length() {
    let state = NetworkState::new_from_yaml(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            ipv4:
              enabled: true
              dhcp: false
              address:
                - ip: 192.0.2.251
                  prefix-length: "24"
        "#,
    )
    .unwrap();

    let iface = state.ifaces.kernel_ifaces.get("eth1").unwrap();
    let ipv4 = iface.base_iface().ipv4.as_ref().unwrap();
    assert_eq!(ipv4.addresses.as_ref().unwrap()[0].prefix_length, 24);
}

#[test]
fn test_yaml_stringified_bool_values() {
    let state = NetworkState::new_from_yaml(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            ipv4:
              enabled: "true"
              dhcp: "yes"
        "#,
    )
    .unwrap();

    let iface = state.ifaces.kernel_ifaces.get("eth1").unwrap();
    let ipv4 = iface.base_iface().ipv4.as_ref().unwrap();
    assert_eq!(ipv4.enabled, Some(true));
    assert_eq!(ipv4.dhcp, Some(true));
}

#[test]
fn test_yaml_stringified_bond_mode_and_option() {
    let state = NetworkState::new_from_yaml(
        r#"---
        interfaces:
          - name: bond0
            type: bond
            bond:
              mode: 4
              options:
                miimon: "140"
        "#,
    )
    .unwrap();

    let iface = state.ifaces.kernel_ifaces.get("bond0").unwrap();
    match iface {
        Interface::Bond(b) => {
            let bond = b.bond.as_ref().unwrap();
            assert_eq!(bond.mode, Some(BondMode::LACP));
            assert_eq!(bond.options.as_ref().unwrap().miimon, Some(140));
        }
        _ => panic!("expected bond interface"),
    }
}

