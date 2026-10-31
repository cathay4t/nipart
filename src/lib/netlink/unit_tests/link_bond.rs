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

