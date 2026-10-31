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

