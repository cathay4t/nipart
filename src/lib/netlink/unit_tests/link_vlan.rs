// SPDX-License-Identifier: Apache-2.0

use rtnetlink::packet_route::link::{
    InfoData, InfoKind, InfoVlan, LinkAttribute, LinkFlags, LinkLayerType,
    LinkMessage, VlanFlags, VlanProtocol as NlVlanProtocol,
    VlanQosMapping as NlQos,
};

use super::fixtures::{link_info_attribute, link_message};
use crate::{
    Interface, InterfaceType, NipartInterface, VlanInterface, VlanProtocol,
    VlanQosMapping, VlanRegistrationProtocol, netlink::link::LinkState,
};

fn parse_vlan_interface(messages: &[LinkMessage]) -> VlanInterface {
    let mut interfaces = LinkState::from_messages(messages).into_interfaces();
    let interface = interfaces
        .drain(..)
        .find(|interface| interface.name() == "vlan100")
        .expect("VLAN interface not found");
    assert_eq!(interface.iface_type(), &InterfaceType::Vlan);
    if let Interface::Vlan(vlan) = interface {
        *vlan
    } else {
        panic!("Not a VLAN interface");
    }
}

fn eth0() -> LinkMessage {
    link_message(2, "eth0", LinkLayerType::Ether, LinkFlags::Up)
}

#[test]
fn test_parse_vlan() {
    let mut message =
        link_message(4, "vlan100", LinkLayerType::Ether, LinkFlags::Up);
    message.attributes.push(LinkAttribute::Link(2));
    message.attributes.push(link_info_attribute(
        InfoKind::Vlan,
        Some(InfoData::Vlan(vec![
            InfoVlan::Id(100),
            InfoVlan::Protocol(NlVlanProtocol::Ieee8021Ad),
            InfoVlan::Flags((
                VlanFlags::Gvrp
                    | VlanFlags::ReorderHdr
                    | VlanFlags::LooseBinding,
                VlanFlags::all(),
            )),
            InfoVlan::IngressQos(vec![NlQos::Mapping(2, 3)]),
            InfoVlan::EgressQos(vec![NlQos::Mapping(4, 5)]),
        ])),
    ));
    let vlan = parse_vlan_interface(&[eth0(), message]);
    assert_eq!(vlan.base.name, "vlan100");
    let config = vlan.vlan.expect("VLAN config missing");
    assert_eq!(config.id, Some(100));
    assert_eq!(config.base_iface.as_deref(), Some("eth0"));
    assert_eq!(config.protocol, Some(VlanProtocol::Ieee8021Ad));
    assert_eq!(
        config.registration_protocol,
        Some(VlanRegistrationProtocol::Gvrp)
    );
    assert_eq!(config.reorder_headers, Some(true));
    assert_eq!(config.loose_binding, Some(true));
    assert_eq!(config.bridge_binding, Some(false));
    assert_eq!(
        config.ingress_qos_map,
        Some(vec![VlanQosMapping { from: 2, to: 3 }])
    );
    assert_eq!(
        config.egress_qos_map,
        Some(vec![VlanQosMapping { from: 4, to: 5 }])
    );
}

