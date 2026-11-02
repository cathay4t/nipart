// SPDX-License-Identifier: Apache-2.0

use std::net::{Ipv4Addr, Ipv6Addr};

use rtnetlink::packet_route::link::{
    BondAdSelect as NlAdSelect, BondAllPortActive as NlAllPortActive,
    BondArpAllTargets as NlArpAllTargets, BondArpValidate as NlArpValidate,
    BondFailOverMac as NlFailOverMac, BondLacpRate as NlLacpRate,
    BondMode as NlBondMode, BondPrimaryReselect as NlPrimaryReselect,
    BondXmitHashPolicy as NlXmitHash, InfoBond, InfoBondPort, InfoData,
    InfoKind, InfoPortData, InfoPortKind, LinkAttribute, LinkFlags, LinkInfo,
    LinkLayerType, LinkMessage,
};

use super::fixtures::{link_info_attribute, link_message};
use crate::{
    BondAllPortActive, BondArpAllTargets, BondArpValidate, BondFailOverMac,
    BondInterface, BondLacpRate, BondMode, BondOptions, BondPrimaryReselect,
    BondXmitHashPolicy, Interface, InterfaceType, NipartInterface,
    netlink::link::LinkState,
};

fn eth_port(
    index: u32,
    name: &str,
    controller: u32,
    bond_port: Vec<InfoBondPort>,
) -> LinkMessage {
    let mut message =
        link_message(index, name, LinkLayerType::Ether, LinkFlags::Up);
    message
        .attributes
        .push(LinkAttribute::Controller(controller));
    message.attributes.push(LinkAttribute::LinkInfo(vec![
        LinkInfo::PortKind(InfoPortKind::Bond),
        LinkInfo::PortData(InfoPortData::BondPort(bond_port)),
    ]));
    message
}

fn parse_bond(messages: &[LinkMessage]) -> BondInterface {
    let mut interfaces = LinkState::from_messages(messages).into_interfaces();
    let interface = interfaces
        .drain(..)
        .find(|interface| interface.name() == "bond0")
        .expect("Bond interface not found");
    assert_eq!(interface.iface_type(), &InterfaceType::Bond);
    if let Interface::Bond(bond) = interface {
        *bond
    } else {
        panic!("Not a bond interface");
    }
}

fn bond_message(index: u32, nlas: Vec<InfoBond>) -> LinkMessage {
    let mut message =
        link_message(index, "bond0", LinkLayerType::Ether, LinkFlags::Up);
    message.attributes.push(link_info_attribute(
        InfoKind::Bond,
        Some(InfoData::Bond(nlas)),
    ));
    message
}

#[test]
fn test_parse_bond_lacp_options() {
    let bond = parse_bond(&[bond_message(
        10,
        vec![
            InfoBond::Mode(NlBondMode::Ieee8023Ad),
            InfoBond::MiiMon(100),
            InfoBond::UpDelay(200),
            InfoBond::DownDelay(300),
            InfoBond::UseCarrier(true),
            InfoBond::ArpInterval(1000),
            InfoBond::ArpIpTarget(vec![
                Ipv4Addr::new(192, 0, 2, 2),
                Ipv4Addr::new(192, 0, 2, 3),
            ]),
            InfoBond::ArpValidate(NlArpValidate::All),
            InfoBond::ArpAllTargets(NlArpAllTargets::All),
            InfoBond::PrimaryReselect(NlPrimaryReselect::Better),
            InfoBond::FailOverMac(NlFailOverMac::Follow),
            InfoBond::XmitHashPolicy(NlXmitHash::Layer34),
            InfoBond::ResendIgmp(2),
            InfoBond::NumPeerNotif(3),
            InfoBond::AllPortsActive(NlAllPortActive::Delivered),
            InfoBond::MinLinks(1),
            InfoBond::LpInterval(10),
            InfoBond::PacketsPerPort(2),
            InfoBond::AdLacpRate(NlLacpRate::Fast),
            InfoBond::AdSelect(NlAdSelect::ActorPortPrio),
            InfoBond::AdActorSysPrio(100),
            InfoBond::AdUserPortKey(7),
            InfoBond::AdActorSystem([2, 0, 0, 0, 0, 0xaa]),
            InfoBond::TlbDynamicLb(true),
            InfoBond::PeerNotifDelay(50),
            InfoBond::AdLacpActive(true),
            InfoBond::MissedMax(2),
            InfoBond::NsIp6Target(vec![
                "2001:db8::1".parse::<Ipv6Addr>().unwrap(),
            ]),
        ],
    )]);
    let config = bond.bond.expect("Bond config missing");
    assert_eq!(config.mode, Some(BondMode::LACP));
    let options = config.options.expect("Bond options missing");
    assert_eq!(options.miimon, Some(100));
    assert_eq!(options.updelay, Some(200));
    assert_eq!(options.downdelay, Some(300));
    assert_eq!(options.use_carrier, Some(true));
    assert_eq!(options.arp_interval, Some(1000));
    assert_eq!(
        options.arp_ip_target.as_deref(),
        Some("192.0.2.2,192.0.2.3")
    );
    assert_eq!(options.arp_validate, Some(BondArpValidate::All));
    assert_eq!(options.arp_all_targets, Some(BondArpAllTargets::All));
    assert_eq!(options.primary_reselect, Some(BondPrimaryReselect::Better));
    assert_eq!(options.fail_over_mac, None);
    assert_eq!(options.xmit_hash_policy, Some(BondXmitHashPolicy::Layer34));
    assert_eq!(options.resend_igmp, None);
    assert_eq!(options.num_unsol_na, None);
    assert_eq!(options.all_ports_active, Some(BondAllPortActive::Delivered));
    assert_eq!(options.min_links, Some(1));
    assert_eq!(options.lp_interval, Some(10));
    assert_eq!(options.packets_per_port, None);
    assert_eq!(options.lacp_rate, Some(BondLacpRate::Fast));
    assert_eq!(options.ad_actor_sys_prio, Some(100));
    assert_eq!(options.ad_user_port_key, Some(7));
    assert_eq!(
        options.ad_actor_system.as_deref(),
        Some("02:00:00:00:00:AA")
    );
    assert_eq!(options.tlb_dynamic_lb, None);
    assert_eq!(options.peer_notif_delay, Some(50));
    assert_eq!(options.lacp_active, Some(true));
    assert_eq!(options.arp_missed_max, None);
    assert_eq!(
        options.ns_ip6_target,
        Some(vec!["2001:db8::1".parse::<Ipv6Addr>().unwrap()])
    );
}

#[test]
fn test_parse_bond_ports_and_primary() {
    let bond = parse_bond(&[
        bond_message(
            10,
            vec![
                InfoBond::Mode(NlBondMode::ActiveBackup),
                InfoBond::Primary(3),
                InfoBond::FailOverMac(NlFailOverMac::Follow),
                InfoBond::NumPeerNotif(4),
                InfoBond::MissedMax(2),
            ],
        ),
        eth_port(
            3,
            "eth1",
            10,
            vec![
                InfoBondPort::Prio(5),
                InfoBondPort::QueueId(2),
                InfoBondPort::PermHwaddr(vec![
                    0x02, 0x00, 0x00, 0x00, 0x00, 0x11,
                ]),
            ],
        ),
        eth_port(2, "eth0", 10, vec![InfoBondPort::Prio(100)]),
    ]);
    let config = bond.bond.expect("Bond config missing");
    assert_eq!(config.mode, Some(BondMode::ActiveBackup));
    let options = config.options.expect("Bond options missing");
    assert_eq!(options.primary.as_deref(), Some("eth1"));
    assert_eq!(options.fail_over_mac, Some(BondFailOverMac::Follow));
    assert_eq!(options.num_unsol_na, Some(4));
    assert_eq!(options.num_grat_arp, Some(4));
    assert_eq!(options.arp_missed_max, Some(2));
    let ports = config.ports.expect("Bond ports missing");
    assert_eq!(ports.len(), 2);
    assert_eq!(ports[0].name, "eth0");
    assert_eq!(ports[0].priority, Some(100));
    assert_eq!(ports[0].queue_id, Some(0));
    assert_eq!(ports[1].name, "eth1");
    assert_eq!(ports[1].priority, Some(5));
    assert_eq!(ports[1].queue_id, Some(2));
}

