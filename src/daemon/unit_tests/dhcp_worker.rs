// SPDX-License-Identifier: Apache-2.0

use nipart::InterfaceType;

use super::*;

fn base_iface_with_auto_route_metric(
    auto_route_metric: Option<i64>,
) -> BaseInterface {
    let mut base_iface =
        BaseInterface::new("eth1".to_string(), InterfaceType::Ethernet);
    base_iface.iface_index = Some(7);
    let mut ipv4 = InterfaceIpv4::default();
    ipv4.enabled = Some(true);
    ipv4.dhcp = Some(true);
    ipv4.auto_route_metric = auto_route_metric;
    base_iface.ipv4 = Some(ipv4);
    base_iface
}

fn lease_with_gateway() -> DhcpV4Lease {
    let mut lease = DhcpV4Lease::default();
    lease.gateways = Some(vec![std::net::Ipv4Addr::new(192, 0, 2, 1)]);
    lease
}

#[test]
fn test_gen_routes_uses_auto_route_metric() {
    let lease = lease_with_gateway();
    let base_iface = base_iface_with_auto_route_metric(Some(321));
    let routes = gen_routes(&lease, &base_iface);
    assert_eq!(routes.config.unwrap()[0].metric, Some(321));
}

#[test]
fn test_gen_routes_falls_back_to_iface_index_metric() {
    let lease = lease_with_gateway();
    let base_iface = base_iface_with_auto_route_metric(None);
    let routes = gen_routes(&lease, &base_iface);
    assert_eq!(routes.config.unwrap()[0].metric, Some(700));
}
