// SPDX-License-Identifier: Apache-2.0

use rtnetlink::packet_route::link::{
    BridgeStpState, InfoBridge, InfoBridgePort, InfoData, InfoPortData,
    VlanProtocol,
};

use super::link_apply::{bridge_iface, info_data, port_data};
use crate::{
    BridgeVlanConfig, BridgeVlanMode, BridgeVlanTrunkTag,
    LinuxBridgeMulticastRouterType, LinuxBridgeOptions, LinuxBridgePortConfig,
    LinuxBridgeStpOptions, VlanProtocol as NipartVlanProtocol,
    netlink::link::bridge,
};

#[test]
fn test_bridge_builder_converts_user_hz_options() {
    let user_hz = bridge::get_user_hz();
    let iface = bridge_iface(
        Some(LinuxBridgeOptions {
            group_addr: Some("01:80:C2:00:00:05".to_string()),
            group_fwd_mask: Some(248),
            mac_ageing_time: Some(1000),
            multicast_router: Some(LinuxBridgeMulticastRouterType::Enabled),
            vlan_protocol: Some(NipartVlanProtocol::Ieee8021Ad),
            vlan_default_pvid: Some(300),
            stp: Some(LinuxBridgeStpOptions {
                enabled: Some(true),
                forward_delay: Some(15),
                hello_time: Some(2),
                max_age: Some(20),
                priority: Some(100),
            }),
            ..Default::default()
        }),
        None,
        None,
    );
    let message = bridge::bridge_link_builder(&iface, None)
        .expect("bridge builder should succeed")
        .build();
    let Some(InfoData::Bridge(infos)) = info_data(&message) else {
        panic!("bridge info data missing");
    };
    assert!(infos.contains(&InfoBridge::GroupAddr([
        0x01, 0x80, 0xC2, 0x00, 0x00, 0x05
    ])));
    assert!(infos.contains(&InfoBridge::GroupFwdMask(248)));
    assert!(infos.contains(&InfoBridge::AgeingTime(1000 * user_hz)));
    assert!(infos.contains(&InfoBridge::ForwardDelay(15 * user_hz)));
    assert!(infos.contains(&InfoBridge::HelloTime(2 * user_hz)));
    assert!(infos.contains(&InfoBridge::MaxAge(20 * user_hz)));
    assert!(infos.contains(&InfoBridge::Priority(100)));
    assert!(
        infos.contains(&InfoBridge::VlanProtocol(VlanProtocol::Ieee8021Ad))
    );
    assert!(infos.contains(&InfoBridge::VlanDefaultPvid(300)));
}

#[test]
fn test_bridge_builder_disables_stp_and_enables_filtering() {
    let iface = bridge_iface(
        Some(LinuxBridgeOptions {
            stp: Some(LinuxBridgeStpOptions {
                enabled: Some(false),
                ..Default::default()
            }),
            ..Default::default()
        }),
        Some(BridgeVlanConfig {
            mode: Some(BridgeVlanMode::Access),
            tag: Some(300),
            ..Default::default()
        }),
        None,
    );
    let message = bridge::bridge_link_builder(&iface, None)
        .expect("bridge builder should succeed")
        .build();
    let Some(InfoData::Bridge(infos)) = info_data(&message) else {
        panic!("bridge info data missing");
    };
    assert!(infos.contains(&InfoBridge::StpState(BridgeStpState::Disabled)));
    assert!(infos.contains(&InfoBridge::VlanFiltering(true)));
}

#[test]
fn test_bridge_vlan_entries_remove_old_entries() {
    let current = BridgeVlanConfig {
        mode: Some(BridgeVlanMode::Trunk),
        trunk_tags: Some(vec![
            BridgeVlanTrunkTag::Id(100),
            BridgeVlanTrunkTag::Id(200),
        ]),
        ..Default::default()
    };
    let desired = BridgeVlanConfig {
        mode: Some(BridgeVlanMode::Trunk),
        trunk_tags: Some(vec![BridgeVlanTrunkTag::Id(200)]),
        ..Default::default()
    };
    let entries = bridge::gen_bridge_vlan_entries(&desired, Some(&current));
    assert!(
        entries
            .iter()
            .any(|entry| entry.vid == Some(100) && entry.remove)
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.vid == Some(200) && !entry.remove)
    );
    assert_eq!(entries.len(), 2);
}

#[test]
fn test_bridge_vlan_messages_split_add_and_remove() {
    let entries = bridge::gen_bridge_vlan_entries(
        &BridgeVlanConfig {
            mode: Some(BridgeVlanMode::Trunk),
            trunk_tags: Some(vec![BridgeVlanTrunkTag::Id(200)]),
            ..Default::default()
        },
        Some(&BridgeVlanConfig {
            mode: Some(BridgeVlanMode::Trunk),
            trunk_tags: Some(vec![BridgeVlanTrunkTag::Id(100)]),
            ..Default::default()
        }),
    );
    assert!(bridge::bridge_vlan_del_message(1, true, &entries).is_some());
    assert!(bridge::bridge_vlan_add_message(1, true, &entries).is_some());
}

#[test]
fn test_bridge_port_message_sets_options() {
    let message = bridge::bridge_port_message(
        7,
        &LinuxBridgePortConfig {
            name: "dummy0".to_string(),
            stp_hairpin_mode: Some(true),
            stp_path_cost: Some(100),
            stp_priority: Some(10),
            vlan: None,
        },
    );
    assert_eq!(message.header.index, 7);
    let Some(InfoPortData::BridgePort(infos)) = port_data(&message) else {
        panic!("bridge port info data missing");
    };
    assert!(infos.contains(&InfoBridgePort::HairpinMode(true)));
    assert!(infos.contains(&InfoBridgePort::Cost(100)));
    assert!(infos.contains(&InfoBridgePort::Priority(10)));
}

#[test]
fn test_bridge_vlan_message_range_flags() {
    use rtnetlink::packet_route::link::{
        AfSpecBridge, BridgeVlanInfo, BridgeVlanInfoFlags, LinkAttribute,
    };

    use crate::netlink::link::bridge::BridgeVlanEntry;

    let entries = vec![BridgeVlanEntry {
        vid: None,
        vid_range: Some((100, 200)),
        is_pvid: false,
        is_egress_untagged: true,
        remove: false,
    }];
    let message = bridge::bridge_vlan_add_message(3, false, &entries)
        .expect("add message should be generated");
    let vlans: Vec<BridgeVlanInfo> = message
        .attributes
        .iter()
        .find_map(|attribute| {
            if let LinkAttribute::AfSpecBridge(infos) = attribute {
                Some(
                    infos
                        .iter()
                        .filter_map(|info| {
                            if let AfSpecBridge::VlanInfo(vlan) = info {
                                Some(*vlan)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>(),
                )
            } else {
                None
            }
        })
        .expect("bridge vlan attributes should exist");
    assert_eq!(vlans.len(), 2);
    assert!(vlans[0].flags.contains(BridgeVlanInfoFlags::RangeBegin));
    assert!(vlans[1].flags.contains(BridgeVlanInfoFlags::RangeEnd));
    assert_eq!(vlans[0].vid, 100);
    assert_eq!(vlans[1].vid, 200);
}
