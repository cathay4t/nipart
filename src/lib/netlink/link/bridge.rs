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
    LinuxBridgeConfig, LinuxBridgeInterface,
    LinuxBridgeMulticastRouterType, LinuxBridgeOptions,
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
    pub(crate) remove: bool,
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

fn multicast_router_from_nl80211(
    value: NlBridgeMulticastRouterType,
) -> LinuxBridgeMulticastRouterType {
    match value {
        NlBridgeMulticastRouterType::Disabled => {
            LinuxBridgeMulticastRouterType::Disabled
        }
        NlBridgeMulticastRouterType::TempQuery => {
            LinuxBridgeMulticastRouterType::Auto
        }
        NlBridgeMulticastRouterType::Permanent => {
            LinuxBridgeMulticastRouterType::Enabled
        }
        _ => {
            log::debug!("Unsupported linux bridge multicast router {value:?}");
            LinuxBridgeMulticastRouterType::Unknown
        }
    }
}

#[derive(Debug, Default, Clone)]
struct InfoBridgeState {
    forward_delay: Option<u32>,
    hello_time: Option<u32>,
    max_age: Option<u32>,
    ageing_time: Option<u32>,
    stp_state: Option<NlBridgeStpState>,
    priority: Option<u16>,
    vlan_filtering: Option<bool>,
    vlan_protocol: Option<VlanProtocol>,
    group_fwd_mask: Option<u16>,
    group_addr: Option<String>,
    multicast_router: Option<LinuxBridgeMulticastRouterType>,
    multicast_snooping: Option<bool>,
    multicast_query_use_ifaddr: Option<bool>,
    multicast_querier: Option<bool>,
    multicast_hash_max: Option<u32>,
    multicast_last_member_count: Option<u32>,
    multicast_startup_query_count: Option<u32>,
    multicast_last_member_interval: Option<u64>,
    multicast_membership_interval: Option<u64>,
    multicast_querier_interval: Option<u64>,
    multicast_query_interval: Option<u64>,
    multicast_query_response_interval: Option<u64>,
    multicast_startup_query_interval: Option<u64>,
    hello_timer: Option<u64>,
    gc_timer: Option<u64>,
    default_pvid: Option<u16>,
}

#[derive(Debug, Default, Clone)]
struct KernelBridgeVlanEntry {
    vid: u16,
    is_pvid: bool,
    is_egress_untagged: bool,
    is_range_start: bool,
    is_range_end: bool,
}

fn kernel_vlan_from_nl80211(info: &BridgeVlanInfo) -> KernelBridgeVlanEntry {
    KernelBridgeVlanEntry {
        vid: info.vid,
        is_pvid: info.flags.contains(BridgeVlanInfoFlags::Pvid),
        is_egress_untagged: info.flags.contains(BridgeVlanInfoFlags::Untagged),
        is_range_start: info.flags.contains(BridgeVlanInfoFlags::RangeBegin),
        is_range_end: info.flags.contains(BridgeVlanInfoFlags::RangeEnd),
    }
}

fn merge_vlan_range(
    kernel_vlans: &[KernelBridgeVlanEntry],
) -> Vec<BridgeVlanEntry> {
    let mut vlans = Vec::new();
    let mut vlan_start = None;
    for kernel_vlan in kernel_vlans {
        match (kernel_vlan.is_range_start, kernel_vlan.is_range_end) {
            (true, false) => {
                vlan_start = Some(kernel_vlan.vid);
                continue;
            }
            (false, true) => {
                if let Some(start) = vlan_start {
                    vlans.push(BridgeVlanEntry {
                        vid: None,
                        vid_range: Some((start, kernel_vlan.vid)),
                        is_pvid: kernel_vlan.is_pvid,
                        is_egress_untagged: kernel_vlan.is_egress_untagged,
                        remove: false,
                    });
                } else {
                    log::warn!(
                        "Invalid kernel bridge VLAN information: missing \
                         start VLAN for {}",
                        kernel_vlan.vid
                    );
                }
                vlan_start = None;
            }
            (false, false) | (true, true) => {
                vlans.push(BridgeVlanEntry {
                    vid: Some(kernel_vlan.vid),
                    vid_range: None,
                    is_pvid: kernel_vlan.is_pvid,
                    is_egress_untagged: kernel_vlan.is_egress_untagged,
                    remove: false,
                });
                vlan_start = None;
            }
        }
    }
    vlans
}

fn is_access_port(entries: &[BridgeVlanEntry]) -> bool {
    entries.len() == 1 && entries[0].is_pvid && entries[0].is_egress_untagged
}

fn vlan_tag_range(entry: &BridgeVlanEntry) -> (u16, u16) {
    entry
        .vid_range
        .unwrap_or((entry.vid.unwrap_or(1), entry.vid.unwrap_or(1)))
}

impl From<LinuxBridgeMulticastRouterType> for NlBridgeMulticastRouterType {
    fn from(value: LinuxBridgeMulticastRouterType) -> Self {
        match value {
            LinuxBridgeMulticastRouterType::Disabled => Self::Disabled,
            LinuxBridgeMulticastRouterType::Auto => Self::TempQuery,
            LinuxBridgeMulticastRouterType::Enabled => Self::Permanent,
            LinuxBridgeMulticastRouterType::Unknown => Self::TempQuery,
        }
    }
}

impl From<LinuxBridgeStpOptions> for NlBridgeStpState {
    fn from(value: LinuxBridgeStpOptions) -> Self {
        if value.enabled == Some(true) {
            Self::KernelStp
        } else {
            Self::Disabled
        }
    }
}

pub(crate) fn bridge_link_builder(
    iface: &LinuxBridgeInterface,
    cur_iface: Option<&LinuxBridgeInterface>,
) -> Result<
    rtnetlink::LinkMessageBuilder<rtnetlink::LinkBridge>,
    crate::NipartError,
> {
    let mut builder = rtnetlink::LinkBridge::new(iface.base.name.as_str());
    let Some(br_conf) = iface.bridge.as_ref() else {
        return Ok(builder);
    };
    if let Some(opts) = br_conf.options.as_ref() {
        if let Some(value) = opts.group_addr.as_deref() {
            let mac = crate::netlink::mac::parse_mac_address(value)?;
            let mac: [u8; 6] = mac.try_into().map_err(|_| {
                crate::NipartError::new(
                    crate::ErrorKind::InvalidArgument,
                    format!(
                        "Invalid bridge group address {value}, expecting \
                         format like 01:80:C2:00:00:00"
                    ),
                )
            })?;
            builder = builder.group_address(mac);
        }
        if let Some(value) = opts.group_fwd_mask.or(opts.group_forward_mask) {
            builder = builder.group_fwd_mask(value);
        }
        if let Some(value) = opts.hash_max {
            builder = builder.mcast_hash_max(value);
        }
        if let Some(value) = opts.mac_ageing_time {
            builder = builder.ageing_time(value * get_user_hz());
        }
        if let Some(value) = opts.multicast_last_member_count {
            builder = builder.mcast_last_member_count(value);
        }
        if let Some(value) = opts.multicast_last_member_interval {
            builder = builder.mcast_last_member_interval(value);
        }
        if let Some(value) = opts.multicast_membership_interval {
            builder = builder.mcast_membership_interval(value);
        }
        if let Some(value) = opts.multicast_querier {
            builder = builder.mcast_querier(value);
        }
        if let Some(value) = opts.multicast_querier_interval {
            builder = builder.mcast_querier_interval(value);
        }
        if let Some(value) = opts.multicast_query_interval {
            builder = builder.mcast_query_interval(value);
        }
        if let Some(value) = opts.multicast_query_response_interval {
            builder = builder.mcast_query_response_interval(value);
        }
        if let Some(value) = opts.multicast_query_use_ifaddr {
            builder = builder.mcast_query_use_ifaddr(value);
        }
        if let Some(value) = opts.multicast_router.clone() {
            builder = builder.mcast_router(value.into());
        }
        if let Some(value) = opts.multicast_snooping {
            builder = builder.mcast_snooping(value);
        }
        if let Some(value) = opts.multicast_startup_query_count {
            builder = builder.mcast_startup_query_count(value);
        }
        if let Some(value) = opts.multicast_startup_query_interval {
            builder = builder.mcast_startup_query_interval(value);
        }
        if let Some(value) = opts.vlan_protocol {
            builder = builder.vlan_protocol(value.into());
        }
        if let Some(value) = opts.vlan_default_pvid {
            builder = builder.vlan_default_pvid(value);
        }
        if let Some(stp) = opts.stp.as_ref() {
            builder = builder.stp_state(stp.clone().into());
            if let Some(value) = stp.forward_delay {
                builder = builder.forward_delay(value as u32 * get_user_hz());
            }
            if let Some(value) = stp.hello_time {
                builder = builder.hello_time(value as u32 * get_user_hz());
            }
            if let Some(value) = stp.max_age {
                builder = builder.max_age(value as u32 * get_user_hz());
            }
            if let Some(value) = stp.priority {
                builder = builder.priority(value);
            }
        }
        if br_conf.vlan.is_some() {
            builder =
                builder.vlan_filtering(iface.vlan_filtering_is_enabled(cur_iface));
        }
    } else if br_conf.vlan.is_some() {
        builder =
            builder.vlan_filtering(iface.vlan_filtering_is_enabled(cur_iface));
    }
    Ok(builder)
}

fn add_bridge_vlan_entries<'a>(
    mut builder: rtnetlink::LinkMessageBuilder<rtnetlink::LinkBridgeVlan>,
    entries: impl Iterator<Item = &'a BridgeVlanEntry>,
) -> rtnetlink::LinkMessageBuilder<rtnetlink::LinkBridgeVlan> {
    use rtnetlink::packet_route::link::BridgeVlanInfoFlags;

    for entry in entries {
        let mut flags = BridgeVlanInfoFlags::empty();
        if entry.is_pvid {
            flags |= BridgeVlanInfoFlags::Pvid;
        }
        if entry.is_egress_untagged {
            flags |= BridgeVlanInfoFlags::Untagged;
        }
        if let Some(vid) = entry.vid {
            builder = builder.vlan(vid, flags);
        } else if let Some((min, max)) = entry.vid_range {
            builder =
                builder.vlan_range_start(min, flags).vlan_range_end(max, flags);
        }
    }
    builder
}

pub(crate) fn bridge_vlan_add_message(
    index: u32,
    bridge_self: bool,
    entries: &[BridgeVlanEntry],
) -> Option<rtnetlink::packet_route::link::LinkMessage> {
    let mut add_entries = entries.iter().filter(|entry| !entry.remove).peekable();
    add_entries.peek()?;
    let mut builder = rtnetlink::LinkBridgeVlan::new(index);
    if bridge_self {
        builder = builder.bridge_self();
    }
    Some(add_bridge_vlan_entries(builder, add_entries).build())
}

pub(crate) fn bridge_vlan_del_message(
    index: u32,
    bridge_self: bool,
    entries: &[BridgeVlanEntry],
) -> Option<rtnetlink::packet_route::link::LinkMessage> {
    let mut del_entries = entries.iter().filter(|entry| entry.remove).peekable();
    del_entries.peek()?;
    let mut builder = rtnetlink::LinkBridgeVlan::new(index);
    if bridge_self {
        builder = builder.bridge_self();
    }
    Some(add_bridge_vlan_entries(builder, del_entries).build())
}

pub(crate) fn bridge_port_message(
    port_index: u32,
    conf: &LinuxBridgePortConfig,
) -> rtnetlink::packet_route::link::LinkMessage {
    let mut builder = rtnetlink::LinkBridgePort::new(port_index);
    if let Some(value) = conf.stp_hairpin_mode {
        builder = builder.hairpin(value);
    }
    if let Some(value) = conf.stp_priority {
        builder = builder.priority(value);
    }
    if let Some(value) = conf.stp_path_cost {
        builder = builder.cost(value);
    }
    builder.build()
}
