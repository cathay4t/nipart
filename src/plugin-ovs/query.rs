// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nmstate project(https://nmstate.io/) which
// is under license of Apache 2.0, authors of nmstate origin file are:
//  * Gris Ge <fge@redhat.com>
//  * Fernando Fernandez Mancera <ffmancera@riseup.net>
//  * Miguel Duarte Barroso <mdbarroso@redhat.com>

use std::collections::HashMap;

use nipart::{
    BaseInterface, Interface, InterfaceType, Interfaces, NetworkState,
    NipartError, NipartInterface, NipartQueryOption, OvsBridgeConfig,
    OvsBridgeInterface, OvsBridgePortConfig, OvsInterface,
};

use super::db::{OvsDbConnection, OvsDbEntry};

pub(crate) async fn query_ovs_state(
    _opt: NipartQueryOption,
    cur_net_state: &NetworkState,
) -> Result<NetworkState, NipartError> {
    let mut net_state = NetworkState::default();

    let mut cli = match OvsDbConnection::new().await {
        Ok(c) => c,
        Err(_) => {
            log::debug!("OVS daemon is not running");
            return Ok(net_state);
        }
    };
    if !cli.check_connection().await {
        log::debug!("OVS daemon is not responding");
        return Ok(net_state);
    }

    let ovsdb_ifaces = cli.get_ovs_ifaces().await?;
    let ovsdb_brs = cli.get_ovs_bridges().await?;
    let ovsdb_ports = cli.get_ovs_ports().await?;

    for ovsdb_br in ovsdb_brs.values() {
        let base_iface = BaseInterface::new(
            ovsdb_br.name.to_string(),
            InterfaceType::OvsBridge,
        );
        let bridge_conf =
            parse_ovs_bridge_conf(ovsdb_br, &ovsdb_ports, &ovsdb_ifaces);
        net_state.ifaces.push(Interface::OvsBridge(Box::new(
            OvsBridgeInterface::new(base_iface, Some(bridge_conf)),
        )));
    }

    let mut port_to_ctrl: HashMap<String, String> = HashMap::new();
    for iface in net_state
        .ifaces
        .user_ifaces
        .values()
        .filter(|i| i.iface_type() == &InterfaceType::OvsBridge)
    {
        if let Some(ports) = iface.ports() {
            for port in ports {
                port_to_ctrl.insert(
                    port.to_string(),
                    iface.kernel_iface_name().to_string(),
                );
            }
        }
    }

    for ovsdb_iface in ovsdb_ifaces.values() {
        fill_ovs_iface(
            ovsdb_iface,
            &port_to_ctrl,
            &mut net_state.ifaces,
            cur_net_state,
        );
    }
    Ok(net_state)
}

fn fill_ovs_iface(
    ovsdb_iface: &OvsDbEntry,
    port_to_ctrl: &HashMap<String, String>,
    ifaces: &mut Interfaces,
    cur_net_state: &NetworkState,
) {
    let Some(ctrl) = port_to_ctrl.get(&ovsdb_iface.name) else {
        log::warn!(
            "Failed to find OVS bridge for interface {}",
            ovsdb_iface.name
        );
        return;
    };

    match ovsdb_iface.iface_type.as_str() {
        "system" => {
            // "system" type OVS interfaces are physical kernel interfaces
            // (e.g. Ethernet, Bond). Copy from current kernel state and
            // set the controller. These kernel interfaces will be replaced
            // during the merge with the full state from the no-daemon query.
            if let Some(cur_iface) =
                cur_net_state.ifaces.kernel_ifaces.get(&ovsdb_iface.name)
            {
                let mut iface = cur_iface.clone_name_type_only();
                iface.base_iface_mut().controller = Some(ctrl.to_string());
                iface.base_iface_mut().controller_type =
                    Some(InterfaceType::OvsBridge);
                ifaces.push(iface);
            }
        }
        "internal" | "patch" | "dpdk" => {
            if matches!(ovsdb_iface.iface_type.as_str(), "patch" | "dpdk") {
                log::info!(
                    "OVS {} is not supported yet",
                    ovsdb_iface.iface_type.as_str()
                );
            }
            if let Some(cur_iface) =
                cur_net_state.ifaces.kernel_ifaces.get(&ovsdb_iface.name)
            {
                let mut iface = cur_iface.clone_name_type_only();
                iface.base_iface_mut().controller = Some(ctrl.to_string());
                iface.base_iface_mut().controller_type =
                    Some(InterfaceType::OvsBridge);
                ifaces.push(iface);
            } else {
                log::debug!(
                    "OVSDB has {} interface {}, creating OvsInterface entry",
                    ovsdb_iface.iface_type.as_str(),
                    ovsdb_iface.name
                );
                let mut base = BaseInterface::new(
                    ovsdb_iface.name.to_string(),
                    InterfaceType::OvsInterface,
                );
                base.controller = Some(ctrl.to_string());
                base.controller_type = Some(InterfaceType::OvsBridge);
                ifaces.push(Interface::OvsInterface(Box::new(
                    OvsInterface::new(base),
                )));
            }
        }
        i => {
            log::debug!("Unknown OVS interface type '{i}'");
        }
    }
}

fn parse_ovs_bridge_conf(
    ovsdb_br: &OvsDbEntry,
    ovsdb_ports: &HashMap<String, OvsDbEntry>,
    ovsdb_ifaces: &HashMap<String, OvsDbEntry>,
) -> OvsBridgeConfig {
    let mut ret = OvsBridgeConfig::default();
    let mut port_confs = Vec::new();
    for port_uuid in ovsdb_br.ports.as_slice() {
        if let Some(ovsdb_port) = ovsdb_ports.get(port_uuid) {
            let mut port_conf = OvsBridgePortConfig::default();
            if ovsdb_port.ports.len() == 1 {
                if let Some(ovsdb_iface) =
                    ovsdb_ifaces.get(ovsdb_port.ports.first().unwrap())
                {
                    port_conf.name.clone_from(&ovsdb_iface.name);
                } else {
                    port_conf.name.clone_from(&ovsdb_port.name);
                }
            } else {
                log::warn!("Not supporting OVS Bond yet");
            }
            port_confs.push(port_conf);
        }
    }
    port_confs.sort_unstable_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
    ret.ports = Some(port_confs);
    ret
}
