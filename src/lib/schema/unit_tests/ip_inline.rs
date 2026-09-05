// SPDX-License-Identifier: Apache-2.0

use std::net::Ipv6Addr;

use super::*;

#[test]
fn test_link_local_removed_from_current() {
    let mut desired = InterfaceIpv6::default();
    let mut current = InterfaceIpv6 {
        enabled: Some(true),
        dhcp: Some(true),
        dhcp_state: None,
        autoconf: Some(true),
        addresses: Some(vec![InterfaceIpAddr {
            ip: IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)),
            prefix_length: 64,
            valid_life_time: None,
            preferred_life_time: None,
        }]),
    };

    desired.sanitize_before_verify(&mut current);

    assert_eq!(current.addresses.unwrap().len(), 0);
}

#[test]
fn test_link_local_removed_from_desired() {
    let link_local_addr = InterfaceIpAddr {
        ip: IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)),
        prefix_length: 64,
        valid_life_time: None,
        preferred_life_time: None,
    };
    let global_addr = InterfaceIpAddr {
        ip: IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
        prefix_length: 64,
        valid_life_time: None,
        preferred_life_time: None,
    };
    let mut desired = InterfaceIpv6 {
        enabled: Some(true),
        dhcp: Some(false),
        dhcp_state: None,
        autoconf: Some(false),
        addresses: Some(vec![link_local_addr, global_addr.clone()]),
    };
    let mut current = InterfaceIpv6::default();

    desired.sanitize_before_verify(&mut current);

    assert_eq!(desired.addresses.unwrap(), vec![global_addr]);
}

#[test]
fn test_non_link_local_kept_in_current() {
    let mut desired = InterfaceIpv6::default();
    let addr = InterfaceIpAddr {
        ip: IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
        prefix_length: 64,
        valid_life_time: None,
        preferred_life_time: None,
    };
    let mut current = InterfaceIpv6 {
        enabled: Some(true),
        dhcp: Some(true),
        dhcp_state: None,
        autoconf: Some(true),
        addresses: Some(vec![addr.clone()]),
    };

    desired.sanitize_before_verify(&mut current);

    assert_eq!(current.addresses.unwrap(), vec![addr]);
}

#[test]
fn test_current_dhcp_none_set_to_false() {
    let mut desired = InterfaceIpv6::default();
    let mut current = InterfaceIpv6 {
        enabled: Some(true),
        dhcp: None,
        dhcp_state: None,
        autoconf: None,
        addresses: Some(vec![]),
    };

    desired.sanitize_before_verify(&mut current);

    assert_eq!(current.dhcp, Some(false));
}

#[test]
fn test_current_addresses_none_set_to_empty() {
    let mut desired = InterfaceIpv6::default();
    let mut current = InterfaceIpv6 {
        enabled: Some(true),
        dhcp: Some(true),
        dhcp_state: None,
        autoconf: Some(true),
        addresses: None,
    };

    desired.sanitize_before_verify(&mut current);

    assert_eq!(current.addresses, Some(vec![]));
}

#[test]
fn test_life_time_sync_from_current() {
    let addr = InterfaceIpAddr {
        ip: IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
        prefix_length: 64,
        valid_life_time: None,
        preferred_life_time: None,
    };
    let cur_addr = InterfaceIpAddr {
        ip: IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
        prefix_length: 64,
        valid_life_time: Some("300sec".into()),
        preferred_life_time: Some("150sec".into()),
    };

    let mut desired = InterfaceIpv6 {
        enabled: Some(true),
        dhcp: None,
        dhcp_state: None,
        autoconf: None,
        addresses: Some(vec![addr.clone()]),
    };
    let mut current = InterfaceIpv6 {
        enabled: Some(true),
        dhcp: None,
        dhcp_state: None,
        autoconf: None,
        addresses: Some(vec![cur_addr.clone()]),
    };

    desired.sanitize_before_verify(&mut current);

    let synced = desired.addresses.unwrap();
    assert_eq!(synced[0].valid_life_time, Some("300sec".into()));
    assert_eq!(synced[0].preferred_life_time, Some("150sec".into()));
}

#[test]
fn test_sorted_desired_addresses_reorder_to_canonical() {
    let addr0 = InterfaceIpAddr {
        ip: IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 2)),
        prefix_length: 64,
        valid_life_time: None,
        preferred_life_time: None,
    };
    let addr1 = InterfaceIpAddr {
        ip: IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
        prefix_length: 64,
        valid_life_time: None,
        preferred_life_time: None,
    };
    let mut desired = InterfaceIpv6 {
        enabled: Some(true),
        dhcp: None,
        dhcp_state: None,
        autoconf: None,
        addresses: Some(vec![addr0.clone(), addr1.clone()]),
    };
    let mut current = InterfaceIpv6 {
        enabled: Some(true),
        dhcp: None,
        dhcp_state: None,
        autoconf: None,
        addresses: Some(vec![]),
    };

    desired.sanitize_before_verify(&mut current);

    let addrs = desired.addresses.unwrap();
    assert_eq!(addrs[0].ip, addr1.ip);
    assert_eq!(addrs[1].ip, addr0.ip);
}

#[test]
fn test_sorted_current_addresses_reorder_to_canonical() {
    let addr0 = InterfaceIpAddr {
        ip: IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 2)),
        prefix_length: 64,
        valid_life_time: None,
        preferred_life_time: None,
    };
    let addr1 = InterfaceIpAddr {
        ip: IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
        prefix_length: 64,
        valid_life_time: None,
        preferred_life_time: None,
    };
    let mut desired = InterfaceIpv6 {
        enabled: Some(true),
        dhcp: None,
        dhcp_state: None,
        autoconf: None,
        addresses: Some(vec![addr1.clone(), addr0.clone()]),
    };
    let mut current = InterfaceIpv6 {
        enabled: Some(true),
        dhcp: None,
        dhcp_state: None,
        autoconf: None,
        addresses: Some(vec![addr0.clone(), addr1.clone()]),
    };

    desired.sanitize_before_verify(&mut current);

    let cur_addrs = current.addresses.unwrap();
    assert_eq!(cur_addrs[0].ip, addr1.ip);
    assert_eq!(cur_addrs[1].ip, addr0.ip);
}

#[test]
fn test_mixed_addresses_link_local_removed() {
    let global_addr = InterfaceIpAddr {
        ip: IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)),
        prefix_length: 64,
        valid_life_time: None,
        preferred_life_time: None,
    };
    let link_local_addr = InterfaceIpAddr {
        ip: IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)),
        prefix_length: 64,
        valid_life_time: None,
        preferred_life_time: None,
    };
    let mut desired = InterfaceIpv6::default();
    let mut current = InterfaceIpv6 {
        enabled: Some(true),
        dhcp: Some(true),
        dhcp_state: None,
        autoconf: Some(true),
        addresses: Some(vec![link_local_addr, global_addr.clone()]),
    };

    desired.sanitize_before_verify(&mut current);

    assert_eq!(current.addresses.unwrap(), vec![global_addr]);
}
