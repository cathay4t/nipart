// SPDX-License-Identifier: Apache-2.0

use rtnetlink::packet_route::link::{
    AfSpecBridge, BridgeMulticastRouterType as NlMcastRouter, BridgeStpState,
    BridgeVlanInfo, BridgeVlanInfoFlags, InfoBridge, InfoBridgePort, InfoData,
    InfoKind, InfoPortData, InfoPortKind, LinkAttribute, LinkFlags, LinkInfo,
    LinkLayerType, LinkMessage, VlanProtocol as NlVlanProtocol,
};

use super::fixtures::{link_info_attribute, link_message};
use crate::{
    BridgeVlanMode, BridgeVlanRange, BridgeVlanTrunkTag, Interface,
    InterfaceType, LinuxBridgeInterface, LinuxBridgeMulticastRouterType,
    NipartInterface, VlanProtocol,
    netlink::link::{LinkState, bridge::get_user_hz},
};

fn bridge_message(
    index: u32,
    infos: Vec<InfoBridge>,
    af_spec: Vec<AfSpecBridge>,
) -> LinkMessage {
    let mut message =
        link_message(index, "br0", LinkLayerType::Ether, LinkFlags::Up);
    message.attributes.push(link_info_attribute(
        InfoKind::Bridge,
        Some(InfoData::Bridge(infos)),
    ));
    message
        .attributes
        .push(LinkAttribute::AfSpecBridge(af_spec));
    message
}

fn parse_bridge(messages: &[LinkMessage]) -> LinuxBridgeInterface {
    let mut interfaces = LinkState::from_messages(messages).into_interfaces();
    let interface = interfaces
        .drain(..)
        .find(|interface| interface.name() == "br0")
        .expect("Bridge interface not found");
    assert_eq!(interface.iface_type(), &InterfaceType::LinuxBridge);
    if let Interface::LinuxBridge(bridge) = interface {
        *bridge
    } else {
        panic!("Not a Linux bridge interface");
    }
}

