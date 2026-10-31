// SPDX-License-Identifier: Apache-2.0

use rtnetlink::packet_route::link::{
    InfoData, InfoKind, InfoPortKind, InfoVrf, LinkAttribute, LinkFlags,
    LinkInfo, LinkLayerType, LinkMessage,
};

use super::fixtures::{link_info_attribute, link_message};
use crate::{
    Interface, InterfaceType, NipartInterface, VrfInterface,
    netlink::link::LinkState,
};

fn parse_vrf(messages: &[LinkMessage]) -> VrfInterface {
    let mut interfaces = LinkState::from_messages(messages).into_interfaces();
    let interface = interfaces
        .drain(..)
        .find(|interface| interface.name() == "vrf0")
        .expect("VRF interface not found");
    assert_eq!(interface.iface_type(), &InterfaceType::Vrf);
    if let Interface::Vrf(vrf) = interface {
        *vrf
    } else {
        panic!("Not a VRF interface");
    }
}

#[test]
fn test_parse_vrf_table_and_ports() {
    let mut vrf = link_message(10, "vrf0", LinkLayerType::Ether, LinkFlags::Up);
    vrf.attributes.push(link_info_attribute(
        InfoKind::Vrf,
        Some(InfoData::Vrf(vec![InfoVrf::TableId(100)])),
    ));
    let mut eth1 = link_message(2, "eth1", LinkLayerType::Ether, LinkFlags::Up);
    eth1.attributes.push(LinkAttribute::Controller(10));
    eth1.attributes
        .push(LinkAttribute::LinkInfo(vec![LinkInfo::PortKind(
            InfoPortKind::Vrf,
        )]));
    let mut eth0 = link_message(3, "eth0", LinkLayerType::Ether, LinkFlags::Up);
    eth0.attributes.push(LinkAttribute::Controller(10));
    eth0.attributes
        .push(LinkAttribute::LinkInfo(vec![LinkInfo::PortKind(
            InfoPortKind::Vrf,
        )]));
    let vrf = parse_vrf(&[vrf, eth1, eth0]);
    let config = vrf.vrf.expect("VRF config missing");
    assert_eq!(config.table_id, Some(100));
    assert_eq!(
        config.ports,
        Some(vec!["eth0".to_string(), "eth1".to_string()])
    );
}

#[test]
fn test_parse_vrf_without_table() {
    let mut vrf = link_message(10, "vrf0", LinkLayerType::Ether, LinkFlags::Up);
    vrf.attributes.push(link_info_attribute(
        InfoKind::Vrf,
        Some(InfoData::Vrf(Vec::new())),
    ));
    let vrf = parse_vrf(&[vrf]);
    let config = vrf.vrf.expect("VRF config missing");
    assert_eq!(config.table_id, Some(0));
    assert_eq!(config.ports, Some(Vec::new()));
}
