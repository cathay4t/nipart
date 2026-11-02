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

