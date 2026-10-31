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

