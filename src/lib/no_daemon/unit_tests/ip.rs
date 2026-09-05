// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, Ipv6Addr};

use super::apply_iface_ip_changes;
use crate::{BaseInterface, InterfaceIpAddr, InterfaceIpv6, InterfaceType};

fn iface_with_ipv6(ipv6: InterfaceIpv6) -> BaseInterface {
    let mut iface =
        BaseInterface::new("eth0".to_string(), InterfaceType::Ethernet);
    iface.ipv6 = Some(ipv6);
    iface
}

fn ipv6_addr(ip: &str, prefix_length: u8) -> InterfaceIpAddr {
    InterfaceIpAddr {
        ip: IpAddr::V6(ip.parse::<Ipv6Addr>().unwrap()),
        prefix_length,
        valid_life_time: None,
        preferred_life_time: None,
    }
}

/// The kernel auto-assigned IPv6 link-local address must not be removed
/// while IPv6 stays enabled, otherwise the DHCPv6 client would lose the
/// source address for its traffic.
#[test]
fn test_link_local_not_removed_when_ipv6_enabled() {
    let des_iface = iface_with_ipv6(InterfaceIpv6 {
        enabled: Some(true),
        dhcp: Some(true),
        addresses: None,
        ..Default::default()
    });
    let cur_iface = iface_with_ipv6(InterfaceIpv6 {
        enabled: Some(true),
        addresses: Some(vec![
            ipv6_addr("fe80::1", 64),
            ipv6_addr("2001:db8::1", 64),
        ]),
        ..Default::default()
    });

    let np_iface =
        apply_iface_ip_changes(&des_iface, Some(&cur_iface)).unwrap();
    let np_addrs = np_iface.unwrap().ipv6.unwrap().addresses;

    // The static global address is purged when switching to DHCP...
    assert!(
        np_addrs
            .iter()
            .any(|a| { a.remove && a.address == "2001:db8::1" })
    );
    // ...but the link-local address is never removed.
    assert!(
        np_addrs
            .iter()
            .all(|a| { !(a.remove && a.address.starts_with("fe80")) })
    );
}

/// When IPv6 is explicitly disabled, the link-local address is purged
/// together with all other IPv6 addresses.
#[test]
fn test_link_local_removed_when_ipv6_disabled() {
    let des_iface = iface_with_ipv6(InterfaceIpv6 {
        enabled: Some(false),
        ..Default::default()
    });
    let cur_iface = iface_with_ipv6(InterfaceIpv6 {
        enabled: Some(true),
        addresses: Some(vec![ipv6_addr("fe80::1", 64)]),
        ..Default::default()
    });

    let np_iface =
        apply_iface_ip_changes(&des_iface, Some(&cur_iface)).unwrap();
    let np_addrs = np_iface.unwrap().ipv6.unwrap().addresses;

    assert!(
        np_addrs
            .iter()
            .any(|a| { a.remove && a.address == "fe80::1" })
    );
}
