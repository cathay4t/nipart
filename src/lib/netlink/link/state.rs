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

impl LinkState {
    pub(crate) fn from_messages(messages: &[LinkMessage]) -> Self {
        let index_map = super::index_map_from_messages(messages.iter());
        let mut ifaces = BTreeMap::new();
        for message in messages {
            if link_name(message).is_none() {
                continue;
            }
            if let Some(iface) = parse_link(message, &index_map) {
                ifaces.insert(message.header.index, iface);
            }
        }
        Self { ifaces, index_map }
    }

    pub(crate) fn apply_wifi(
        &mut self,
        iface_name: &str,
        config: Option<WifiConfig>,
    ) {
        if let Some(iface) = self
            .ifaces
            .values_mut()
            .find(|iface| iface.base.name == iface_name)
        {
            iface.base.iface_type = InterfaceType::WifiPhy;
            iface.is_wifi = true;
            iface.wifi = config;
        }
    }

    pub(crate) fn apply_ethtool(
        &mut self,
        et_links: HashMap<String, EthernetConfig>,
    ) {
        for iface in self.ifaces.values_mut() {
            if !matches!(
                iface.base.iface_type,
                InterfaceType::Ethernet | InterfaceType::Veth
            ) {
                continue;
            }
            if let Some(config) = et_links.get(iface.base.name.as_str()) {
                iface.ethernet = Some(config.clone());
            }
        }
    }

    pub(crate) fn apply_wireguard(
        &mut self,
        wg_links: HashMap<String, WireguardConfig>,
    ) {
        for iface in self.ifaces.values_mut() {
            if iface.base.iface_type != InterfaceType::Wireguard {
                continue;
            }
            if let Some(config) = wg_links.get(iface.base.name.as_str()) {
                iface.wireguard = Some(config.clone());
            }
        }
    }

    pub(crate) fn wireguard_names(&self) -> Vec<String> {
        self.ifaces
            .values()
            .filter(|iface| iface.base.iface_type == InterfaceType::Wireguard)
            .map(|iface| iface.base.name.clone())
            .collect()
    }

    pub(crate) fn into_interfaces(mut self) -> Vec<Interface> {
        self.resolve_references();
        self.fill_ports();
        self.ifaces
            .into_values()
            .filter(|iface| !should_skip(iface))
            .map(LinkIface::into_interface)
            .collect()
    }

    fn resolve_references(&mut self) {
        let index_map = &self.index_map;
        let index_names: BTreeMap<u32, String> = self
            .ifaces
            .iter()
            .map(|(index, iface)| (*index, iface.base.name.clone()))
            .collect();
        for iface in self.ifaces.values_mut() {
            if let Some(peer_index) = iface.veth_peer_index {
                let peer = if iface.link_netnsid.is_some() {
                    peer_index.to_string()
                } else {
                    index_map
                        .name(peer_index)
                        .map(str::to_string)
                        .unwrap_or_else(|| peer_index.to_string())
                };
                iface.veth = Some(VethConfig { peer });
            }
            if let Some(vlan) = iface.vlan.as_mut() {
                vlan.base_iface = Some(resolve_base_index(
                    index_map,
                    iface.vlan_base_index,
                    vlan.base_iface.as_deref(),
                ));
            }
            if let Some(vxlan) = iface.vxlan.as_mut() {
                vxlan.base_iface = Some(resolve_base_index(
                    index_map,
                    iface.vxlan_base_index,
                    vxlan.base_iface.as_deref(),
                ));
            }
            let primary_index = iface.bond_primary_index;
            if let Some(primary_index) = primary_index
                && let Some(options) =
                    iface.bond.as_mut().and_then(|bond| bond.options.as_mut())
            {
                options.primary = Some(
                    index_names
                        .get(&primary_index)
                        .cloned()
                        .unwrap_or_else(|| primary_index.to_string()),
                );
            }
        }
    }

    fn fill_ports(&mut self) {
        let mut bond_ports: BTreeMap<String, Vec<BondPortConfig>> =
            BTreeMap::new();
        let mut bridge_ports: BTreeMap<String, Vec<LinuxBridgePortConfig>> =
            BTreeMap::new();
        let mut vrf_ports: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut port_vlans: BTreeMap<(String, String), BridgeVlanConfig> =
            BTreeMap::new();

        for iface in self.ifaces.values() {
            let Some(controller) = iface.base.controller.as_ref() else {
                continue;
            };
            match iface.controller_type {
                Some(InfoPortKind::Bond) => {
                    let mut port = iface
                        .bond_port
                        .clone()
                        .unwrap_or_else(empty_bond_port_config);
                    port.name = iface.base.name.clone();
                    bond_ports
                        .entry(controller.clone())
                        .or_default()
                        .push(port);
                }
                Some(InfoPortKind::Bridge) => {
                    let mut port =
                        iface.bridge_port.clone().unwrap_or_default();
                    port.name = iface.base.name.clone();
                    if let Some(vlans) = iface.bridge_port_vlans.as_deref()
                        && let Some(bridge) = self
                            .ifaces
                            .values()
                            .find(|iface| iface.base.name == *controller)
                        && bridge.bridge_filtering == Some(true)
                        && let Some(config) = bridge_vlan_config(
                            vlans,
                            bridge.bridge_default_pvid,
                        )
                    {
                        port_vlans.insert(
                            (controller.clone(), port.name.clone()),
                            config,
                        );
                    }
                    bridge_ports
                        .entry(controller.clone())
                        .or_default()
                        .push(port);
                }
                Some(InfoPortKind::Vrf) => {
                    vrf_ports
                        .entry(controller.clone())
                        .or_default()
                        .push(iface.base.name.clone());
                }
                _ => (),
            }
        }

        for iface in self.ifaces.values_mut() {
            let name = iface.base.name.clone();
            match iface.base.iface_type {
                InterfaceType::Bond => {
                    if let Some(mut ports) = bond_ports.remove(&name) {
                        ports.sort_by(|a, b| a.name.cmp(&b.name));
                        if let Some(bond) = iface.bond.as_mut() {
                            bond.ports = Some(ports);
                        }
                    }
                }
                InterfaceType::LinuxBridge => {
                    if let Some(mut ports) = bridge_ports.remove(&name) {
                        ports.sort_by(|a, b| a.name.cmp(&b.name));
                        for port in ports.iter_mut() {
                            port.vlan = port_vlans
                                .remove(&(name.clone(), port.name.clone()));
                        }
                        if let Some(bridge) = iface.bridge.as_mut() {
                            bridge.ports = Some(ports);
                        }
                    }
                    if iface.bridge_filtering == Some(true) {
                        let default_pvid = iface.bridge_default_pvid;
                        if let Some(vlans) = iface.bridge_vlans.as_deref()
                            && let Some(bridge) = iface.bridge.as_mut()
                        {
                            bridge.vlan =
                                bridge_vlan_config(vlans, default_pvid);
                        }
                    }
                }
                InterfaceType::Vrf => {
                    if let Some(mut ports) = vrf_ports.remove(&name) {
                        ports.sort();
                        if let Some(vrf) = iface.vrf.as_mut() {
                            vrf.ports = Some(ports);
                        }
                    }
                }
                _ => (),
            }
        }
    }
}

fn empty_bond_port_config() -> BondPortConfig {
    BondPortConfig {
        name: String::new(),
        priority: None,
        queue_id: None,
    }
}

