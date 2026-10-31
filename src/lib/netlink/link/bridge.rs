// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nispor project
// (https://github.com/nispor/nispor) and nmstate project
// (https://nmstate.io/) which are under license of Apache 2.0.

use rtnetlink::packet_route::link::{
    AfSpecBridge, BridgeMulticastRouterType as NlBridgeMulticastRouterType,
    BridgeStpState as NlBridgeStpState, BridgeVlanInfo, BridgeVlanInfoFlags,
    InfoBridge, InfoBridgePort, InfoData, VlanProtocol as NlVlanProtocol,
};

use crate::{
    BridgeVlanConfig, BridgeVlanMode, BridgeVlanRange, BridgeVlanTrunkTag,
    LinuxBridgeConfig, LinuxBridgeMulticastRouterType, LinuxBridgeOptions,
    LinuxBridgePortConfig, LinuxBridgeStpOptions, VlanProtocol,
    netlink::mac::format_mac_address,
};

const DEFAULT_USER_HZ: u32 = 100;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BridgeVlanEntry {
    pub(crate) vid: Option<u16>,
    pub(crate) vid_range: Option<(u16, u16)>,
    pub(crate) is_pvid: bool,
    pub(crate) is_egress_untagged: bool,
}

pub(crate) fn parse_bridge(
    data: &InfoData,
) -> Option<(LinuxBridgeConfig, Option<bool>, Option<u16>)> {
    let InfoData::Bridge(infos) = data else {
        return None;
    };
    let mut bridge = InfoBridgeState::default();
    for info in infos {
        match info {
            InfoBridge::ForwardDelay(value) => {
                bridge.forward_delay = Some(*value)
            }
            InfoBridge::HelloTime(value) => bridge.hello_time = Some(*value),
            InfoBridge::MaxAge(value) => bridge.max_age = Some(*value),
            InfoBridge::AgeingTime(value) => bridge.ageing_time = Some(*value),
            InfoBridge::StpState(value) => bridge.stp_state = Some(*value),
            InfoBridge::Priority(value) => bridge.priority = Some(*value),
            InfoBridge::VlanFiltering(value) => {
                bridge.vlan_filtering = Some(*value)
            }
            InfoBridge::VlanProtocol(value) => {
                bridge.vlan_protocol = Some(vlan_protocol_from_nl80211(*value))
            }
            InfoBridge::GroupFwdMask(value) => {
                bridge.group_fwd_mask = Some(*value)
            }
            InfoBridge::GroupAddr(mac) => {
                bridge.group_addr = format_mac_address(mac)
            }
            InfoBridge::MulticastRouter(value) => {
                bridge.multicast_router =
                    Some(multicast_router_from_nl80211(*value))
            }
            InfoBridge::MulticastSnooping(value) => {
                bridge.multicast_snooping = Some(*value)
            }
            InfoBridge::MulticastQueryUseIfaddr(value) => {
                bridge.multicast_query_use_ifaddr = Some(*value)
            }
            InfoBridge::MulticastQuerier(value) => {
                bridge.multicast_querier = Some(*value)
            }
            InfoBridge::MulticastHashMax(value) => {
                bridge.multicast_hash_max = Some(*value)
            }
            InfoBridge::MulticastLastMemberCount(value) => {
                bridge.multicast_last_member_count = Some(*value)
            }
            InfoBridge::MulticastStartupQueryCount(value) => {
                bridge.multicast_startup_query_count = Some(*value)
            }
            InfoBridge::MulticastLastMemberInterval(value) => {
                bridge.multicast_last_member_interval = Some(*value)
            }
            InfoBridge::MulticastMembershipInterval(value) => {
                bridge.multicast_membership_interval = Some(*value)
            }
            InfoBridge::MulticastQuerierInterval(value) => {
                bridge.multicast_querier_interval = Some(*value)
            }
            InfoBridge::MulticastQueryInterval(value) => {
                bridge.multicast_query_interval = Some(*value)
            }
            InfoBridge::MulticastQueryResponseInterval(value) => {
                bridge.multicast_query_response_interval = Some(*value)
            }
            InfoBridge::MulticastStartupQueryInterval(value) => {
                bridge.multicast_startup_query_interval = Some(*value)
            }
            InfoBridge::HelloTimer(value) => bridge.hello_timer = Some(*value),
            InfoBridge::GcTimer(value) => bridge.gc_timer = Some(*value),
            InfoBridge::VlanDefaultPvid(value) => {
                bridge.default_pvid = Some(*value)
            }
            _ => (),
        }
    }
    let stp = Some(get_stp_options(&bridge));
    let options = LinuxBridgeOptions {
        gc_timer: bridge.gc_timer,
        group_addr: bridge.group_addr,
        group_forward_mask: bridge.group_fwd_mask,
        group_fwd_mask: bridge.group_fwd_mask,
        hash_max: bridge.multicast_hash_max,
        hello_timer: bridge.hello_timer,
        mac_ageing_time: bridge.ageing_time.map(devide_by_user_hz),
        multicast_last_member_count: bridge.multicast_last_member_count,
        multicast_last_member_interval: bridge.multicast_last_member_interval,
        multicast_membership_interval: bridge.multicast_membership_interval,
        multicast_querier: bridge.multicast_querier,
        multicast_querier_interval: bridge.multicast_querier_interval,
        multicast_query_interval: bridge.multicast_query_interval,
        multicast_query_response_interval: bridge
            .multicast_query_response_interval,
        multicast_query_use_ifaddr: bridge.multicast_query_use_ifaddr,
        multicast_router: bridge.multicast_router.clone(),
        multicast_snooping: bridge.multicast_snooping,
        multicast_startup_query_count: bridge.multicast_startup_query_count,
        multicast_startup_query_interval: bridge
            .multicast_startup_query_interval,
        stp,
        vlan_protocol: bridge.vlan_protocol,
        vlan_default_pvid: bridge.default_pvid,
    };
    Some((
        LinuxBridgeConfig {
            options: Some(options),
            ports: Some(Vec::new()),
            vlan: None,
        },
        bridge.vlan_filtering,
        bridge.default_pvid,
    ))
}

pub(crate) fn parse_bridge_port(
    nlas: &[InfoBridgePort],
) -> LinuxBridgePortConfig {
    let mut ret = LinuxBridgePortConfig {
        name: String::new(),
        stp_hairpin_mode: Some(false),
        stp_path_cost: Some(0),
        stp_priority: Some(0),
        vlan: None,
    };
    for nla in nlas {
        match nla {
            InfoBridgePort::HairpinMode(value) => {
                ret.stp_hairpin_mode = Some(*value)
            }
            InfoBridgePort::Cost(value) => ret.stp_path_cost = Some(*value),
            InfoBridgePort::Priority(value) => ret.stp_priority = Some(*value),
            _ => (),
        }
    }
    ret
}

pub(crate) fn parse_af_spec_bridge(
    nlas: &[AfSpecBridge],
) -> Option<Vec<BridgeVlanEntry>> {
    let mut vlans = Vec::new();
    for nla in nlas {
        if let AfSpecBridge::VlanInfo(info) = nla {
            vlans.push(kernel_vlan_from_nl80211(info));
        }
    }
    if vlans.is_empty() {
        None
    } else {
        Some(merge_vlan_range(&vlans))
    }
}

pub(crate) fn bridge_vlan_config(
    entries: &[BridgeVlanEntry],
    default_pvid: Option<u16>,
) -> Option<BridgeVlanConfig> {
    let mut ret = BridgeVlanConfig::default();
    let mut is_native = false;
    let mut trunk_tags = Vec::new();
    let is_access_port = is_access_port(entries);

    for entry in entries {
        let default_pvid = default_pvid.unwrap_or(1);
        let (vlan_min, vlan_max) = vlan_tag_range(entry);
        if vlan_min == default_pvid && vlan_max == default_pvid {
            continue;
        }
        if is_access_port {
            ret.tag = Some(vlan_min);
        } else if entry.is_pvid && entry.is_egress_untagged {
            ret.tag = Some(vlan_max);
            is_native = true;
        } else if vlan_min == vlan_max {
            trunk_tags.push(BridgeVlanTrunkTag::Id(vlan_min));
        } else {
            trunk_tags.push(BridgeVlanTrunkTag::IdRange(BridgeVlanRange {
                max: vlan_max,
                min: vlan_min,
            }));
        }
    }
    if trunk_tags.is_empty() {
        ret.mode = Some(BridgeVlanMode::Access);
    } else {
        ret.mode = Some(BridgeVlanMode::Trunk);
        ret.enable_native = Some(is_native);
    }
    if ret.mode == Some(BridgeVlanMode::Access)
        && trunk_tags.is_empty()
        && ret.tag.is_none()
    {
        None
    } else {
        ret.trunk_tags = Some(trunk_tags);
        Some(ret)
    }
}

pub(crate) fn get_user_hz() -> u32 {
    if let Ok(Some(user_hz)) =
        nix::unistd::sysconf(nix::unistd::SysconfVar::CLK_TCK)
        && user_hz > 0
    {
        user_hz as u32
    } else {
        DEFAULT_USER_HZ
    }
}

// The kernel is multiplying these bridge properties by USER_HZ, we should
// divide into seconds:
//   * forward_delay
//   * ageing_time
//   * hello_time
//   * max_age
//
// When kernel CONFIG_HZ is not multiple times of USER_HZ, we will notice 1
// different between desired value and active value. For example, Arch Linux
// has CONFIG_HZ=300 which causing ageing_time requested 600 got 599 in
// return. To fix the verification error, we add 1 for this trivial
// difference.
fn devide_by_user_hz(value: u32) -> u32 {
    (value + 1) / get_user_hz()
}

fn get_stp_options(bridge: &InfoBridgeState) -> LinuxBridgeStpOptions {
    LinuxBridgeStpOptions {
        enabled: Some(matches!(
            bridge.stp_state,
            Some(NlBridgeStpState::KernelStp) | Some(NlBridgeStpState::UserStp)
        )),
        forward_delay: bridge.forward_delay.map(devide_by_user_hz).map(|v| {
            u8::try_from(v).unwrap_or(LinuxBridgeStpOptions::FORWARD_DELAY_MAX)
        }),
        max_age: bridge.max_age.map(devide_by_user_hz).map(|v| {
            u8::try_from(v).unwrap_or(LinuxBridgeStpOptions::MAX_AGE_MAX)
        }),
        hello_time: bridge.hello_time.map(devide_by_user_hz).map(|v| {
            u8::try_from(v).unwrap_or(LinuxBridgeStpOptions::HELLO_TIME_MAX)
        }),
        priority: bridge.priority,
    }
}

fn vlan_protocol_from_nl80211(protocol: NlVlanProtocol) -> VlanProtocol {
    match protocol {
        NlVlanProtocol::Ieee8021Q => VlanProtocol::Ieee8021Q,
        NlVlanProtocol::Ieee8021Ad => VlanProtocol::Ieee8021Ad,
        _ => VlanProtocol::Unknown,
    }
}

