// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nispor project
// (https://github.com/nispor/nispor) and nmstate project
// (https://nmstate.io/) which are under license of Apache 2.0.

use rtnetlink::packet_route::link::{
    InfoData, InfoKind, InfoPortData, InfoPortKind, LinkAttribute, LinkFlags,
    LinkInfo, LinkLayerType, LinkMessage, Prop, State,
};

use super::{
    LinkIndexMap,
    bond::parse_bond,
    bridge::{self, BridgeVlanEntry, parse_bridge},
    vlan::parse_vlan,
    vrf::parse_vrf,
    vxlan::parse_vxlan,
};
use crate::{
    BaseInterface, BondConfig, BondPortConfig, EthernetConfig,
    InterfaceLinkState, InterfaceState, InterfaceType, LinuxBridgeConfig,
    LinuxBridgePortConfig, VethConfig, VlanConfig, VrfConfig, VxlanConfig,
    WifiConfig, WireguardConfig,
    netlink::{mac::format_mac_address, sysfs},
};

#[derive(Debug, Default, Clone)]
pub(crate) struct LinkIface {
    pub(crate) base: BaseInterface,
    pub(crate) veth: Option<VethConfig>,
    pub(crate) vlan: Option<VlanConfig>,
    pub(crate) vrf: Option<VrfConfig>,
    pub(crate) vxlan: Option<VxlanConfig>,
    pub(crate) bond: Option<BondConfig>,
    pub(crate) bond_port: Option<BondPortConfig>,
    pub(crate) bridge: Option<LinuxBridgeConfig>,
    pub(crate) bridge_port: Option<LinuxBridgePortConfig>,
    pub(crate) ethernet: Option<EthernetConfig>,
    pub(crate) wireguard: Option<WireguardConfig>,
    pub(crate) wifi: Option<WifiConfig>,
    pub(crate) is_wifi: bool,
    pub(crate) bond_port_perm_mac: Option<String>,
    pub(crate) bond_primary_index: Option<u32>,
    pub(crate) controller_type: Option<InfoPortKind>,
    pub(crate) veth_peer_index: Option<u32>,
    pub(crate) vlan_base_index: Option<u32>,
    pub(crate) vxlan_base_index: Option<u32>,
    pub(crate) link_netnsid: Option<i32>,
    pub(crate) bridge_filtering: Option<bool>,
    pub(crate) bridge_default_pvid: Option<u16>,
    pub(crate) bridge_vlans: Option<Vec<BridgeVlanEntry>>,
    pub(crate) bridge_port_vlans: Option<Vec<BridgeVlanEntry>>,
}

