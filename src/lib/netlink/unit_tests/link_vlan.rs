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

