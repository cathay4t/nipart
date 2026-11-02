// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nispor project
// (https://github.com/nispor/nispor) which is under license of Apache 2.0.

use rtnetlink::packet_route::link::{
    BondAdSelect as NlBondAdSelect, BondAllPortActive as NlBondAllPortActive,
    BondArpAllTargets as NlBondArpAllTargets,
    BondArpValidate as NlBondArpValidate, BondFailOverMac as NlBondFailOverMac,
    BondLacpRate as NlBondLacpRate, BondMode as NlBondMode,
    BondPrimaryReselect as NlBondPrimaryReselect,
    BondXmitHashPolicy as NlBondXmitHashPolicy, InfoBond, InfoBondPort,
    InfoData,
};

use crate::{
    BondAdSelect, BondAllPortActive, BondArpAllTargets, BondArpValidate,
    BondConfig, BondFailOverMac, BondLacpRate, BondMode, BondOptions,
    BondPortConfig, BondPrimaryReselect, BondXmitHashPolicy,
    netlink::mac::format_mac_address,
};

#[derive(Debug, Default)]
pub(crate) struct ParsedBondPort {
    pub(crate) config: BondPortConfig,
    pub(crate) permanent_mac: Option<String>,
}

pub(crate) fn parse_bond(data: &InfoData) -> Option<(BondConfig, Option<u32>)> {
    let InfoData::Bond(nlas) = data else {
        return None;
    };
    let mode = nlas
        .iter()
        .find_map(|nla| {
            if let InfoBond::Mode(mode) = nla {
                Some(bond_mode_from_nl80211(*mode))
            } else {
                None
            }
        })
        .unwrap_or_default();
    let mut primary_index = None;
    let mut options = BondOptions::default();
    for nla in nlas {
        match nla {
            InfoBond::Mode(_) => (),
            InfoBond::MiiMon(value) => options.miimon = Some(*value),
            InfoBond::UpDelay(value) => options.updelay = Some(*value),
            InfoBond::DownDelay(value) => options.downdelay = Some(*value),
            InfoBond::UseCarrier(value) => options.use_carrier = Some(*value),
            InfoBond::ArpInterval(value) => options.arp_interval = Some(*value),
            InfoBond::ArpIpTarget(addresses) => {
                options.arp_ip_target = Some(
                    addresses
                        .iter()
                        .map(|address| address.to_string())
                        .collect::<Vec<String>>()
                        .join(","),
                );
            }
            InfoBond::ArpValidate(value) => {
                options.arp_validate = bond_arp_validate_from_nl80211(*value);
            }
            InfoBond::ArpAllTargets(value) => {
                options.arp_all_targets =
                    bond_arp_all_targets_from_nl80211(*value);
            }
            InfoBond::Primary(value)
                if [BondMode::ActiveBackup, BondMode::ALB, BondMode::TLB]
                    .contains(&mode) =>
            {
                primary_index = Some(*value);
            }
            InfoBond::PrimaryReselect(value) => {
                options.primary_reselect =
                    bond_primary_reselect_from_nl80211(*value);
            }
            InfoBond::FailOverMac(value) if mode == BondMode::ActiveBackup => {
                options.fail_over_mac = bond_fail_over_mac_from_nl80211(*value);
            }
            InfoBond::XmitHashPolicy(value)
                if [BondMode::XOR, BondMode::LACP, BondMode::TLB]
                    .contains(&mode) =>
            {
                options.xmit_hash_policy =
                    bond_xmit_hash_policy_from_nl80211(*value);
            }
            InfoBond::ResendIgmp(value)
                if [
                    BondMode::RoundRobin,
                    BondMode::ActiveBackup,
                    BondMode::TLB,
                    BondMode::ALB,
                ]
                .contains(&mode) =>
            {
                options.resend_igmp = Some(*value);
            }
            InfoBond::NumPeerNotif(value) if mode == BondMode::ActiveBackup => {
                options.num_unsol_na = Some(*value);
                options.num_grat_arp = Some(*value);
            }
            InfoBond::AllPortsActive(value) => {
                options.all_ports_active =
                    bond_all_ports_active_from_nl80211(*value);
            }
            InfoBond::MinLinks(value) => options.min_links = Some(*value),
            InfoBond::LpInterval(value) => options.lp_interval = Some(*value),
            InfoBond::PacketsPerPort(value) if mode == BondMode::RoundRobin => {
                options.packets_per_port = Some(*value);
            }
            InfoBond::AdLacpRate(value) if mode == BondMode::LACP => {
                options.lacp_rate = bond_lacp_rate_from_nl80211(*value);
            }
            InfoBond::AdSelect(value) if mode == BondMode::LACP => {
                options.ad_select = bond_ad_select_from_nl80211(*value);
            }
            InfoBond::AdActorSysPrio(value) if mode == BondMode::LACP => {
                options.ad_actor_sys_prio = Some(*value);
            }
            InfoBond::AdUserPortKey(value) if mode == BondMode::LACP => {
                options.ad_user_port_key = Some(*value);
            }
            InfoBond::AdActorSystem(mac) if mode == BondMode::LACP => {
                options.ad_actor_system = format_mac_address(mac);
            }
            InfoBond::TlbDynamicLb(value)
                if [BondMode::TLB, BondMode::ALB].contains(&mode) =>
            {
                options.tlb_dynamic_lb = Some(*value);
            }
            InfoBond::PeerNotifDelay(value) => {
                options.peer_notif_delay = Some(*value)
            }
            InfoBond::AdLacpActive(value) if mode == BondMode::LACP => {
                options.lacp_active = Some(*value);
            }
            InfoBond::MissedMax(value)
                if ![BondMode::LACP, BondMode::TLB, BondMode::ALB]
                    .contains(&mode) =>
            {
                options.arp_missed_max = Some(*value);
            }
            InfoBond::NsIp6Target(addresses) => {
                options.ns_ip6_target = Some(addresses.clone());
            }
            _ => {
                log::debug!("Unsupported bond info {nla:?}");
            }
        }
    }
    Some((
        BondConfig {
            mode: Some(mode),
            options: Some(options),
            ports: Some(Vec::new()),
        },
        primary_index,
    ))
}

pub(crate) fn parse_bond_port(nlas: &[InfoBondPort]) -> ParsedBondPort {
    let mut priority = 0i32;
    let mut queue_id = 0u16;
    let mut permanent_mac = None;
    for nla in nlas {
        match nla {
            InfoBondPort::Prio(value) => priority = *value,
            InfoBondPort::QueueId(value) => queue_id = *value,
            InfoBondPort::PermHwaddr(mac) => {
                permanent_mac = format_mac_address(mac);
            }
            _ => (),
        }
    }
    ParsedBondPort {
        config: BondPortConfig {
            name: String::new(),
            priority: Some(priority),
            queue_id: Some(queue_id),
        },
        permanent_mac,
    }
}

fn bond_mode_from_nl80211(mode: NlBondMode) -> BondMode {
    match mode {
        NlBondMode::BalanceRr => BondMode::RoundRobin,
        NlBondMode::ActiveBackup => BondMode::ActiveBackup,
        NlBondMode::BalanceXor => BondMode::XOR,
        NlBondMode::Broadcast => BondMode::Broadcast,
        NlBondMode::Ieee8023Ad => BondMode::LACP,
        NlBondMode::BalanceTlb => BondMode::TLB,
        NlBondMode::BalanceAlb => BondMode::ALB,
        _ => BondMode::Unknown,
    }
}

fn bond_arp_all_targets_from_nl80211(
    value: NlBondArpAllTargets,
) -> Option<BondArpAllTargets> {
    match value {
        NlBondArpAllTargets::Any => Some(BondArpAllTargets::Any),
        NlBondArpAllTargets::All => Some(BondArpAllTargets::All),
        _ => {
            log::warn!("Unsupported bond arp_all_targets option {value:?}");
            None
        }
    }
}

fn bond_arp_validate_from_nl80211(
    value: NlBondArpValidate,
) -> Option<BondArpValidate> {
    match value {
        NlBondArpValidate::None => Some(BondArpValidate::None),
        NlBondArpValidate::Active => Some(BondArpValidate::Active),
        NlBondArpValidate::Backup => Some(BondArpValidate::Backup),
        NlBondArpValidate::All => Some(BondArpValidate::All),
        NlBondArpValidate::FilterActive => Some(BondArpValidate::FilterActive),
        NlBondArpValidate::FilterBackup => Some(BondArpValidate::FilterBackup),
        _ => {
            log::warn!("Unsupported bond arp_validate option {value:?}");
            None
        }
    }
}

fn bond_primary_reselect_from_nl80211(
    value: NlBondPrimaryReselect,
) -> Option<BondPrimaryReselect> {
    match value {
        NlBondPrimaryReselect::Always => Some(BondPrimaryReselect::Always),
        NlBondPrimaryReselect::Better => Some(BondPrimaryReselect::Better),
        NlBondPrimaryReselect::Failure => Some(BondPrimaryReselect::Failure),
        _ => {
            log::warn!("Unsupported bond primary_reselect option {value:?}");
            None
        }
    }
}

fn bond_fail_over_mac_from_nl80211(
    value: NlBondFailOverMac,
) -> Option<BondFailOverMac> {
    match value {
        NlBondFailOverMac::None => Some(BondFailOverMac::None),
        NlBondFailOverMac::Active => Some(BondFailOverMac::Active),
        NlBondFailOverMac::Follow => Some(BondFailOverMac::Follow),
        _ => {
            log::warn!("Unsupported bond fail_over_mac option {value:?}");
            None
        }
    }
}

fn bond_xmit_hash_policy_from_nl80211(
    value: NlBondXmitHashPolicy,
) -> Option<BondXmitHashPolicy> {
    match value {
        NlBondXmitHashPolicy::Layer2 => Some(BondXmitHashPolicy::Layer2),
        NlBondXmitHashPolicy::Layer34 => Some(BondXmitHashPolicy::Layer34),
        NlBondXmitHashPolicy::Layer23 => Some(BondXmitHashPolicy::Layer23),
        NlBondXmitHashPolicy::Encap23 => Some(BondXmitHashPolicy::Encap23),
        NlBondXmitHashPolicy::Encap34 => Some(BondXmitHashPolicy::Encap34),
        NlBondXmitHashPolicy::VlanSrcMac => {
            Some(BondXmitHashPolicy::VlanSrcMac)
        }
        _ => {
            log::warn!("Unsupported bond xmit_hash_policy option {value:?}");
            None
        }
    }
}

fn bond_all_ports_active_from_nl80211(
    value: NlBondAllPortActive,
) -> Option<BondAllPortActive> {
    match value {
        NlBondAllPortActive::Dropped => Some(BondAllPortActive::Dropped),
        NlBondAllPortActive::Delivered => Some(BondAllPortActive::Delivered),
        _ => {
            log::warn!("Unsupported bond all_ports_active option {value:?}");
            None
        }
    }
}

fn bond_lacp_rate_from_nl80211(value: NlBondLacpRate) -> Option<BondLacpRate> {
    match value {
        NlBondLacpRate::Slow => Some(BondLacpRate::Slow),
        NlBondLacpRate::Fast => Some(BondLacpRate::Fast),
        _ => {
            log::warn!("Unsupported bond lacp_rate option {value:?}");
            None
        }
    }
}

fn bond_ad_select_from_nl80211(value: NlBondAdSelect) -> Option<BondAdSelect> {
    match value {
        NlBondAdSelect::Stable => Some(BondAdSelect::Stable),
        NlBondAdSelect::Bandwidth => Some(BondAdSelect::Bandwidth),
        NlBondAdSelect::Count => Some(BondAdSelect::Count),
        NlBondAdSelect::ActorPortPrio => Some(BondAdSelect::ActorPortPrio),
        _ => {
            log::warn!("Unsupported bond ad_select option {value:?}");
            None
        }
    }
}

impl From<BondMode> for NlBondMode {
    fn from(value: BondMode) -> Self {
        match value {
            BondMode::RoundRobin => Self::BalanceRr,
            BondMode::ActiveBackup => Self::ActiveBackup,
            BondMode::XOR => Self::BalanceXor,
            BondMode::Broadcast => Self::Broadcast,
            BondMode::LACP => Self::Ieee8023Ad,
            BondMode::TLB => Self::BalanceTlb,
            BondMode::ALB => Self::BalanceAlb,
            BondMode::Unknown => Self::ActiveBackup,
        }
    }
}

impl From<BondArpAllTargets> for NlBondArpAllTargets {
    fn from(value: BondArpAllTargets) -> Self {
        match value {
            BondArpAllTargets::All => Self::All,
            BondArpAllTargets::Any => Self::Any,
        }
    }
}

impl From<BondArpValidate> for NlBondArpValidate {
    fn from(value: BondArpValidate) -> Self {
        match value {
            BondArpValidate::None => Self::None,
            BondArpValidate::Active => Self::Active,
            BondArpValidate::Backup => Self::Backup,
            BondArpValidate::All => Self::All,
            BondArpValidate::Filter => Self::Filter,
            BondArpValidate::FilterActive => Self::FilterActive,
            BondArpValidate::FilterBackup => Self::FilterBackup,
        }
    }
}

impl From<BondPrimaryReselect> for NlBondPrimaryReselect {
    fn from(value: BondPrimaryReselect) -> Self {
        match value {
            BondPrimaryReselect::Always => Self::Always,
            BondPrimaryReselect::Better => Self::Better,
            BondPrimaryReselect::Failure => Self::Failure,
        }
    }
}

impl From<BondFailOverMac> for NlBondFailOverMac {
    fn from(value: BondFailOverMac) -> Self {
        match value {
            BondFailOverMac::None => Self::None,
            BondFailOverMac::Active => Self::Active,
            BondFailOverMac::Follow => Self::Follow,
        }
    }
}

impl From<BondXmitHashPolicy> for NlBondXmitHashPolicy {
    fn from(value: BondXmitHashPolicy) -> Self {
        match value {
            BondXmitHashPolicy::Layer2 => Self::Layer2,
            BondXmitHashPolicy::Layer34 => Self::Layer34,
            BondXmitHashPolicy::Layer23 => Self::Layer23,
            BondXmitHashPolicy::Encap23 => Self::Encap23,
            BondXmitHashPolicy::Encap34 => Self::Encap34,
            BondXmitHashPolicy::VlanSrcMac => Self::VlanSrcMac,
        }
    }
}

impl From<BondAllPortActive> for NlBondAllPortActive {
    fn from(value: BondAllPortActive) -> Self {
        match value {
            BondAllPortActive::Dropped => Self::Dropped,
            BondAllPortActive::Delivered => Self::Delivered,
        }
    }
}

impl From<BondLacpRate> for NlBondLacpRate {
    fn from(value: BondLacpRate) -> Self {
        match value {
            BondLacpRate::Fast => Self::Fast,
            BondLacpRate::Slow => Self::Slow,
        }
    }
}

impl From<BondAdSelect> for NlBondAdSelect {
    fn from(value: BondAdSelect) -> Self {
        match value {
            BondAdSelect::Stable => Self::Stable,
            BondAdSelect::Bandwidth => Self::Bandwidth,
            BondAdSelect::Count => Self::Count,
            BondAdSelect::ActorPortPrio => Self::ActorPortPrio,
        }
    }
}

pub(crate) async fn bond_link_builder(
    handle: &rtnetlink::Handle,
    iface: &crate::BondInterface,
    cur_iface: Option<&crate::BondInterface>,
) -> Result<
    rtnetlink::LinkMessageBuilder<rtnetlink::LinkBond>,
    crate::NipartError,
> {
    let mut builder = rtnetlink::LinkBond::new(iface.base.name.as_str());
    let Some(bond_conf) = iface.bond.as_ref() else {
        return Ok(builder);
    };
    if let Some(mode) = bond_conf.mode
        && cur_iface.is_none()
    {
        builder = builder.mode(mode.into());
    }
    let Some(opts) = bond_conf.options.as_ref() else {
        return Ok(builder);
    };
    if let Some(v) = opts.miimon {
        builder = builder.miimon(v);
    }
    if let Some(v) = opts.updelay {
        builder = builder.updelay(v);
    }
    if let Some(v) = opts.downdelay {
        builder = builder.downdelay(v);
    }
    if let Some(v) = opts.use_carrier {
        builder = builder.use_carrier(v);
    }
    if let Some(v) = opts.arp_interval {
        builder = builder.arp_interval(v);
    }
    if let Some(v) = opts.arp_ip_target.as_deref() {
        let mut addresses = Vec::new();
        for address in v.split(',') {
            addresses.push(
                address.parse::<std::net::Ipv4Addr>().map_err(|_| {
                    crate::NipartError::new(
                        crate::ErrorKind::InvalidArgument,
                        format!(
                            "Invalid bond `arp_ip_target` property, should \
                             be IPv4 addresses separated by ',', but got {v}"
                        ),
                    )
                })?,
            );
        }
        builder = builder.arp_ip_target(addresses);
    }
    if let Some(v) = opts.arp_all_targets.clone() {
        builder = builder.arp_all_targets(v.into());
    }
    if let Some(v) = opts.arp_validate.clone() {
        builder = builder.arp_validate(v.into());
    }
    if let Some(v) = opts.primary.as_deref() {
        builder = builder.primary(super::get_iface_index(handle, v).await?);
    }
    if let Some(v) = opts.primary_reselect.clone() {
        builder = builder.primary_reselect(v.into());
    }
    if let Some(v) = opts.fail_over_mac {
        builder = builder.fail_over_mac(v.into());
    }
    if let Some(v) = opts.xmit_hash_policy {
        builder = builder.xmit_hash_policy(v.into());
    }
    if let Some(v) = opts.resend_igmp {
        builder = builder.resend_igmp(v);
    }
    if let Some(v) = opts.num_unsol_na.or(opts.num_grat_arp) {
        builder = builder.num_peer_notif(v);
    }
    if let Some(v) = opts.all_ports_active.clone() {
        builder = builder.all_ports_active(v.into());
    }
    if let Some(v) = opts.min_links {
        builder = builder.min_links(v);
    }
    if let Some(v) = opts.lp_interval {
        builder = builder.lp_interval(v);
    }
    if let Some(v) = opts.packets_per_port {
        builder = builder.packets_per_port(v);
    }
    if let Some(v) = opts.lacp_rate {
        builder = builder.ad_lacp_rate(v.into());
    }
    if let Some(v) = opts.ad_select {
        builder = builder.ad_select(v.into());
    }
    if let Some(v) = opts.ad_actor_sys_prio {
        builder = builder.ad_actor_sys_prio(v);
    }
    if let Some(v) = opts.ad_user_port_key {
        builder = builder.ad_user_port_key(v);
    }
    if let Some(v) = opts.ad_actor_system.as_deref() {
        let mac = crate::netlink::mac::parse_mac_address(v)?;
        let mac: [u8; 6] = mac.try_into().map_err(|_| {
            crate::NipartError::new(
                crate::ErrorKind::InvalidArgument,
                format!(
                    "Invalid MAC address length for bond \
                     ad_actor_system {v}, should be like 02:00:00:00:00:01"
                ),
            )
        })?;
        builder = builder.ad_actor_system(mac);
    }
    if let Some(v) = opts.tlb_dynamic_lb {
        builder = builder.tlb_dynamic_lb(v);
    }
    if let Some(v) = opts.peer_notif_delay {
        builder = builder.peer_notif_delay(v);
    }
    if let Some(v) = opts.lacp_active {
        builder = builder.ad_lacp_active(v);
    }
    if let Some(v) = opts.arp_missed_max {
        builder = builder.missed_max(v);
    }
    if let Some(v) = opts.ns_ip6_target.as_ref() {
        builder = builder.ns_ip6_target(v.clone());
    }
    Ok(builder)
}

/// Changing the bond mode requires detaching every port, changing the mode,
/// then reattaching the ports which existed before the apply.
pub(crate) async fn bond_mode_change_messages(
    handle: &rtnetlink::Handle,
    iface: &crate::BondInterface,
    cur_iface: &crate::BondInterface,
    existing_iface_names: &std::collections::HashSet<String>,
) -> Result<Vec<rtnetlink::packet_route::link::LinkMessage>, crate::NipartError>
{
    let Some(des_mode) = iface.bond.as_ref().and_then(|b| b.mode) else {
        return Ok(Vec::new());
    };
    let Some(cur_mode) = cur_iface.bond.as_ref().and_then(|b| b.mode) else {
        return Ok(Vec::new());
    };
    if des_mode == cur_mode {
        return Ok(Vec::new());
    }

    let bond_name = iface.base.name.as_str();
    let bond_index = match cur_iface.base.iface_index {
        Some(index) => index,
        None => super::get_iface_index(handle, bond_name).await?,
    };

    let mut ret = Vec::new();
    if let Some(ports) = cur_iface.bond.as_ref().and_then(|b| b.ports.as_ref())
    {
        for port in ports {
            ret.push(
                rtnetlink::LinkUnspec::new_with_name(port.name.as_str())
                    .nocontroller()
                    .up()
                    .build(),
            );
        }
    }
    ret.push(
        rtnetlink::LinkBond::new(bond_name)
            .mode(des_mode.into())
            .build(),
    );
    if let Some(ports) = iface
        .bond
        .as_ref()
        .and_then(|b| b.ports.as_ref())
        .or_else(|| cur_iface.bond.as_ref().and_then(|b| b.ports.as_ref()))
    {
        for port in ports {
            if existing_iface_names.contains(port.name.as_str()) {
                ret.push(
                    rtnetlink::LinkUnspec::new_with_name(port.name.as_str())
                        .controller(bond_index)
                        .up()
                        .build(),
                );
            }
        }
    }
    Ok(ret)
}
