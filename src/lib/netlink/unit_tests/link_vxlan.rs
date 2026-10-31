// SPDX-License-Identifier: Apache-2.0

use std::net::Ipv4Addr;

use rtnetlink::packet_route::link::{
    InfoData, InfoKind, InfoVxlan, LinkAttribute, LinkFlags, LinkLayerType,
};

use super::fixtures::{link_info_attribute, link_message};
use crate::{
    Interface, InterfaceType, NipartInterface, VxlanInterface,
    netlink::link::LinkState,
};

fn parse_vxlan() -> VxlanInterface {
    let mut message =
        link_message(20, "vxlan0", LinkLayerType::Ether, LinkFlags::Up);
    message.attributes.push(LinkAttribute::Link(2));
    message.attributes.push(link_info_attribute(
        InfoKind::Vxlan,
        Some(InfoData::Vxlan(vec![
            InfoVxlan::Id(42),
            InfoVxlan::Group(Ipv4Addr::new(192, 0, 2, 1)),
            InfoVxlan::Local(Ipv4Addr::new(192, 0, 2, 2)),
            InfoVxlan::Ttl(64),
            InfoVxlan::Tos(10),
            InfoVxlan::Learning(false),
            InfoVxlan::Ageing(300),
            InfoVxlan::Limit(1000),
            InfoVxlan::PortRange((5000, 6000)),
            InfoVxlan::Proxy(true),
            InfoVxlan::Rsc(true),
            InfoVxlan::Port(4789),
            InfoVxlan::UDPCsum(true),
            InfoVxlan::Gpe,
        ])),
    ));
    let eth0 = link_message(2, "eth0", LinkLayerType::Ether, LinkFlags::Up);
    let mut interfaces =
        LinkState::from_messages(&[message, eth0]).into_interfaces();
    let interface = interfaces
        .drain(..)
        .find(|interface| interface.name() == "vxlan0")
        .expect("VXLAN interface not found");
    assert_eq!(interface.iface_type(), &InterfaceType::Vxlan);
    if let Interface::Vxlan(vxlan) = interface {
        *vxlan
    } else {
        panic!("Not a VXLAN interface");
    }
}

#[test]
fn test_parse_vxlan() {
    let vxlan = parse_vxlan();
    let config = vxlan.vxlan.expect("VXLAN config missing");
    assert_eq!(config.id, Some(42));
    assert_eq!(config.base_iface.as_deref(), Some("eth0"));
    assert_eq!(config.remote.as_deref(), Some("192.0.2.1"));
    assert_eq!(config.local.as_deref(), Some("192.0.2.2"));
    assert_eq!(config.ttl, Some(64));
    assert_eq!(config.tos, Some(10));
    assert_eq!(config.learning, Some(false));
    assert_eq!(config.ageing, Some(300));
    assert_eq!(config.max_address, Some(1000));
    assert_eq!(config.src_port_min, Some(5000));
    assert_eq!(config.src_port_max, Some(6000));
    assert_eq!(config.proxy, Some(true));
    assert_eq!(config.rsc, Some(true));
    assert_eq!(config.destination_port, Some(4789));
    assert_eq!(config.udp_check_sum, Some(true));
    assert_eq!(config.gpe, Some(true));
    assert_eq!(config.gbp, Some(false));
    assert_eq!(config.collect_metadata, Some(false));
}
