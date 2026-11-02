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

#[test]
fn test_parse_bridge_options_and_stp() {
    let bridge = parse_bridge(&[bridge_message(
        10,
        vec![
            InfoBridge::ForwardDelay(1500),
            InfoBridge::HelloTime(200),
            InfoBridge::MaxAge(2000),
            InfoBridge::AgeingTime(30000),
            InfoBridge::StpState(BridgeStpState::KernelStp),
            InfoBridge::Priority(32768),
            InfoBridge::GroupFwdMask(0),
            InfoBridge::MulticastRouter(NlMcastRouter::TempQuery),
            InfoBridge::MulticastSnooping(true),
            InfoBridge::VlanFiltering(false),
            InfoBridge::VlanProtocol(NlVlanProtocol::Ieee8021Q),
            InfoBridge::VlanDefaultPvid(1),
            InfoBridge::GcTimer(123),
            InfoBridge::HelloTimer(456),
        ],
        Vec::new(),
    )]);
    let config = bridge.bridge.expect("Bridge config missing");
    let options = config.options.expect("Bridge options missing");
    let user_hz = get_user_hz();
    assert_eq!(options.mac_ageing_time, Some((30000 + 1) / user_hz));
    assert_eq!(options.gc_timer, Some(123));
    assert_eq!(options.hello_timer, Some(456));
    assert_eq!(options.group_fwd_mask, Some(0));
    assert_eq!(options.group_forward_mask, Some(0));
    assert_eq!(
        options.multicast_router,
        Some(LinuxBridgeMulticastRouterType::Auto)
    );
    assert_eq!(options.multicast_snooping, Some(true));
    assert_eq!(options.vlan_protocol, Some(VlanProtocol::Ieee8021Q));
    assert_eq!(options.vlan_default_pvid, Some(1));
    let stp = options.stp.expect("STP options missing");
    assert_eq!(stp.enabled, Some(true));
    assert_eq!(stp.forward_delay, Some((1500 / user_hz) as u8));
    assert_eq!(stp.hello_time, Some((200 / user_hz) as u8));
    assert_eq!(stp.max_age, Some((2000 / user_hz) as u8));
    assert_eq!(stp.priority, Some(32768));
    assert_eq!(config.vlan, None);
}

#[test]
fn test_parse_bridge_ports_and_vlans() {
    let mut port = link_message(2, "eth1", LinkLayerType::Ether, LinkFlags::Up);
    port.attributes.push(LinkAttribute::Controller(10));
    port.attributes.push(LinkAttribute::LinkInfo(vec![
        LinkInfo::PortKind(InfoPortKind::Bridge),
        LinkInfo::PortData(InfoPortData::BridgePort(vec![
            InfoBridgePort::HairpinMode(true),
            InfoBridgePort::Cost(100),
            InfoBridgePort::Priority(32),
        ])),
    ]));
    port.attributes.push(LinkAttribute::AfSpecBridge(vec![
        AfSpecBridge::VlanInfo(BridgeVlanInfo {
            flags: BridgeVlanInfoFlags::Pvid | BridgeVlanInfoFlags::Untagged,
            vid: 100,
        }),
        AfSpecBridge::VlanInfo(BridgeVlanInfo {
            flags: BridgeVlanInfoFlags::Untagged,
            vid: 200,
        }),
    ]));
    let bridge = parse_bridge(&[
        bridge_message(
            10,
            vec![
                InfoBridge::VlanFiltering(true),
                InfoBridge::VlanDefaultPvid(1),
            ],
            vec![AfSpecBridge::VlanInfo(BridgeVlanInfo {
                flags: BridgeVlanInfoFlags::Pvid
                    | BridgeVlanInfoFlags::Untagged,
                vid: 10,
            })],
        ),
        port,
    ]);
    let config = bridge.bridge.expect("Bridge config missing");
    let bridge_vlan = config.vlan.expect("Bridge VLAN config missing");
    assert_eq!(bridge_vlan.mode, Some(BridgeVlanMode::Access));
    assert_eq!(bridge_vlan.tag, Some(10));
    let ports = config.ports.expect("Bridge ports missing");
    assert_eq!(ports.len(), 1);
    assert_eq!(ports[0].name, "eth1");
    assert_eq!(ports[0].stp_hairpin_mode, Some(true));
    assert_eq!(ports[0].stp_path_cost, Some(100));
    assert_eq!(ports[0].stp_priority, Some(32));
    let port_vlan = ports[0].vlan.clone().expect("Port VLAN config missing");
    assert_eq!(port_vlan.mode, Some(BridgeVlanMode::Trunk));
    assert_eq!(port_vlan.tag, Some(100));
    assert_eq!(port_vlan.enable_native, Some(true));
    assert_eq!(
        port_vlan.trunk_tags,
        Some(vec![BridgeVlanTrunkTag::Id(200)])
    );
}

#[test]
fn test_parse_bridge_vlan_range() {
    let bridge = parse_bridge(&[bridge_message(
        10,
        vec![
            InfoBridge::VlanFiltering(true),
            InfoBridge::VlanDefaultPvid(1),
        ],
        vec![
            AfSpecBridge::VlanInfo(BridgeVlanInfo {
                flags: BridgeVlanInfoFlags::RangeBegin,
                vid: 100,
            }),
            AfSpecBridge::VlanInfo(BridgeVlanInfo {
                flags: BridgeVlanInfoFlags::RangeEnd,
                vid: 200,
            }),
        ],
    )]);
    let config = bridge.bridge.expect("Bridge config missing");
    let bridge_vlan = config.vlan.expect("Bridge VLAN config missing");
    assert_eq!(bridge_vlan.mode, Some(BridgeVlanMode::Trunk));
    assert_eq!(
        bridge_vlan.trunk_tags,
        Some(vec![BridgeVlanTrunkTag::IdRange(BridgeVlanRange {
            min: 100,
            max: 200,
        })])
    );
}
