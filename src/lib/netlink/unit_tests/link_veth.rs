// SPDX-License-Identifier: Apache-2.0

use rtnetlink::packet_route::link::{
    InfoKind, LinkAttribute, LinkFlags, LinkLayerType, LinkMessage,
};

use super::fixtures::{link_info_attribute, link_message};
use crate::{
    EthernetInterface, Interface, InterfaceType, NipartInterface,
    netlink::link::LinkState,
};

fn veth(index: u32, name: &str, peer: u32) -> LinkMessage {
    let mut message =
        link_message(index, name, LinkLayerType::Ether, LinkFlags::Up);
    message.attributes.push(LinkAttribute::Link(peer));
    message
        .attributes
        .push(link_info_attribute(InfoKind::Veth, None));
    message
}

fn ethernet_by_name(
    interfaces: Vec<Interface>,
    name: &str,
) -> EthernetInterface {
    let interface = interfaces
        .into_iter()
        .find(|interface| interface.name() == name)
        .unwrap_or_else(|| panic!("Interface {name} not found"));
    assert_eq!(interface.iface_type(), &InterfaceType::Veth);
    if let Interface::Ethernet(ethernet) = interface {
        *ethernet
    } else {
        panic!("Not an ethernet interface");
    }
}

