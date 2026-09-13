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

fn routes_from_yaml(yaml: &str) -> Routes {
    rmsd_yaml::from_str(yaml).unwrap()
}

#[test]
fn test_stale_lease_gateway_route_is_marked_absent() {
    // A lease learned from another network left its gateway route behind:
    // it shares destination and metric with the new lease's route, so the
    // route apply would refuse the new route without an explicit removal.
    let cur_routes = routes_from_yaml(
        r#"---
        running:
          - destination: 0.0.0.0/0
            next-hop-interface: eth1
            next-hop-address: 192.0.2.1
            metric: 700
            table-id: 254
        "#,
    );
    let mut routes = routes_from_yaml(
        r#"---
        config:
          - destination: 0.0.0.0/0
            next-hop-interface: eth1
            next-hop-address: 198.51.100.1
            metric: 700
            table-id: 254
        "#,
    );

    mark_replaced_routes_absent(&mut routes, &cur_routes);

    let config_routes = routes.config.unwrap();
    assert_eq!(config_routes.len(), 2);
    let absent_route = config_routes
        .iter()
        .find(|rt| rt.is_absent())
        .expect("stale gateway route was not marked absent");
    assert_eq!(absent_route.next_hop_addr.as_deref(), Some("192.0.2.1"));
}

#[test]
fn test_unchanged_lease_gateway_route_is_kept() {
    let cur_routes = routes_from_yaml(
        r#"---
        running:
          - destination: 0.0.0.0/0
            next-hop-interface: eth1
            next-hop-address: 192.0.2.1
            metric: 700
            table-id: 254
        "#,
    );
    let mut routes = routes_from_yaml(
        r#"---
        config:
          - destination: 0.0.0.0/0
            next-hop-interface: eth1
            next-hop-address: 192.0.2.1
            metric: 700
            table-id: 254
        "#,
    );

    mark_replaced_routes_absent(&mut routes, &cur_routes);

    let config_routes = routes.config.unwrap();
    assert_eq!(config_routes.len(), 1);
    assert!(!config_routes[0].is_absent());
}

#[test]
fn test_route_of_other_iface_or_metric_is_kept() {
    let cur_routes = routes_from_yaml(
        r#"---
        running:
          - destination: 0.0.0.0/0
            next-hop-interface: eth2
            next-hop-address: 192.0.2.1
            metric: 700
            table-id: 254
          - destination: 0.0.0.0/0
            next-hop-interface: eth1
            next-hop-address: 192.0.2.1
            metric: 701
            table-id: 254
        "#,
    );
    let mut routes = routes_from_yaml(
        r#"---
        config:
          - destination: 0.0.0.0/0
            next-hop-interface: eth1
            next-hop-address: 198.51.100.1
            metric: 700
            table-id: 254
        "#,
    );

    mark_replaced_routes_absent(&mut routes, &cur_routes);

    let config_routes = routes.config.unwrap();
    assert_eq!(config_routes.len(), 1);
    assert!(!config_routes[0].is_absent());
}
