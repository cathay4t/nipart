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

#[test]
fn test_parse_veth_peer() {
    let interfaces =
        LinkState::from_messages(&[veth(5, "veth0", 6), veth(6, "veth1", 5)])
            .into_interfaces();
    let veth0 = ethernet_by_name(interfaces.clone(), "veth0");
    assert_eq!(
        veth0.veth.as_ref().map(|veth| veth.peer.as_str()),
        Some("veth1")
    );
    let veth1 = ethernet_by_name(interfaces, "veth1");
    assert_eq!(
        veth1.veth.as_ref().map(|veth| veth.peer.as_str()),
        Some("veth0")
    );
}

