// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};

use rtnetlink::packet_route::link::{InfoPortKind, LinkMessage};

use super::{
    LinkIndexMap,
    bridge::bridge_vlan_config,
    parse::{LinkIface, link_name, parse_link},
};
use crate::{
    BondPortConfig, BridgeVlanConfig, DummyInterface, EthernetConfig,
    EthernetInterface, Interface, InterfaceType, LinuxBridgeInterface,
    LinuxBridgePortConfig, LoopbackInterface, OvsInterface, UnknownInterface,
    VethConfig, VlanInterface, VrfInterface, VxlanInterface, WifiConfig,
    WifiPhyInterface, WireguardConfig, WireguardInterface,
};

#[derive(Debug, Default)]
pub(crate) struct LinkState {
    ifaces: BTreeMap<u32, LinkIface>,
    index_map: LinkIndexMap,
}

