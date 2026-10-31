// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nmstate project(https://nmstate.io/) which
// is under license of Apache 2.0, authors of original file
// (rust/src/lib/nispor/linux_bridge.rs) are:
//  * Gris Ge <fge@redhat.com>
//  * Fernando Fernandez Mancera <ffmancera@riseup.net>
//  * Íñigo Huguet <ihuguet@redhat.com>

use super::linux_bridge_vlan::gen_bridge_vlan_conf;
use crate::{
    ErrorKind, LinuxBridgeInterface, LinuxBridgeMulticastRouterType,
    LinuxBridgePortConfig, NipartError,
};

impl From<LinuxBridgeMulticastRouterType>
    for nispor::BridgeMulticastRouterType
{
    fn from(v: LinuxBridgeMulticastRouterType) -> Self {
        match v {
            LinuxBridgeMulticastRouterType::Disabled => Self::Disabled,
            LinuxBridgeMulticastRouterType::Auto => Self::TempQuery,
            LinuxBridgeMulticastRouterType::Enabled => Self::Perm,
            _ => {
                log::debug!(
                    "Unsupported linux bridge multicast router type {v:?}, \
                     treating as auto"
                );
                Self::TempQuery
            }
        }
    }
}

pub(crate) fn apply_bridge_conf(
    mut np_iface: nispor::IfaceConf,
    iface: &LinuxBridgeInterface,
    cur_iface: Option<&LinuxBridgeInterface>,
) -> Result<Vec<nispor::IfaceConf>, NipartError> {
    let mut np_br = nispor::BridgeConf::default();
    if let Some(br_opts) =
        iface.bridge.as_ref().and_then(|b| b.options.as_ref())
    {
        if let Some(v) = br_opts.group_addr.as_ref() {
            np_br.group_address = Some(parse_eth_mac(v.as_str())?);
        }
        if let Some(v) = br_opts.group_forward_mask.as_ref() {
            np_br.group_fwd_mask = Some(*v);
        }
        if let Some(v) = br_opts.group_fwd_mask.as_ref() {
            np_br.group_fwd_mask = Some(*v);
        }
        if let Some(v) = br_opts.hash_max.as_ref() {
            np_br.mcast_hash_max = Some(*v);
        }
        if let Some(v) = br_opts.mac_ageing_time.as_ref() {
            np_br.ageing_time =
                Some(*v * crate::netlink::link::bridge::get_user_hz());
        }
        if let Some(v) = br_opts.multicast_last_member_count.as_ref() {
            np_br.mcast_last_member_count = Some(*v);
        }
        if let Some(v) = br_opts.multicast_last_member_interval.as_ref() {
            np_br.mcast_last_member_interval = Some(*v);
        }
        if let Some(v) = br_opts.multicast_membership_interval.as_ref() {
            np_br.mcast_membership_interval = Some(*v);
        }
        if let Some(v) = br_opts.multicast_querier.as_ref() {
            np_br.mcast_querier = Some(*v);
        }
        if let Some(v) = br_opts.multicast_querier_interval.as_ref() {
            np_br.mcast_querier_interval = Some(*v);
        }
        if let Some(v) = br_opts.multicast_query_interval.as_ref() {
            np_br.mcast_query_interval = Some(*v);
        }
        if let Some(v) = br_opts.multicast_query_response_interval.as_ref() {
            np_br.mcast_query_response_interval = Some(*v);
        }
        if let Some(v) = br_opts.multicast_query_use_ifaddr.as_ref() {
            np_br.mcast_query_use_ifaddr = Some(*v);
        }
        if let Some(v) = br_opts.multicast_router.as_ref() {
            np_br.mcast_router = Some(v.clone().into());
        }
        if let Some(v) = br_opts.multicast_snooping.as_ref() {
            np_br.mcast_snooping = Some(*v);
        }
        if let Some(v) = br_opts.multicast_startup_query_count.as_ref() {
            np_br.mcast_startup_query_count = Some(*v);
        }
        if let Some(v) = br_opts.multicast_startup_query_interval.as_ref() {
            np_br.mcast_startup_query_interval = Some(*v);
        }
        if let Some(v) = br_opts.vlan_protocol.as_ref() {
            np_br.vlan_protocol = Some((*v).into());
        }
        if let Some(v) = br_opts.vlan_default_pvid.as_ref() {
            np_br.vlan_default_pvid = Some(*v);
        }
        if let Some(stp) = br_opts.stp.as_ref() {
            np_br.stp_state = stp.enabled.map(|enabled| {
                if enabled {
                    nispor::BridgeStpState::KernelStp
                } else {
                    nispor::BridgeStpState::Disabled
                }
            });

            np_br.forward_delay = stp.forward_delay.map(|v| {
                v as u32 * crate::netlink::link::bridge::get_user_hz()
            });

            np_br.hello_time = stp.hello_time.map(|v| {
                v as u32 * crate::netlink::link::bridge::get_user_hz()
            });
            np_br.max_age = stp.max_age.map(|v| {
                v as u32 * crate::netlink::link::bridge::get_user_hz()
            });
            np_br.priority = stp.priority;
        }
    }
    if let Some(vlan_conf) = iface
        .bridge
        .as_ref()
        .and_then(|br_conf| br_conf.vlan.as_ref())
    {
        np_br.vlans = Some(gen_bridge_vlan_conf(
            vlan_conf,
            cur_iface
                .and_then(|c| c.bridge.as_ref())
                .and_then(|cur_br_conf| cur_br_conf.vlan.as_ref()),
        ));
    }
    np_br.vlan_filtering = Some(iface.vlan_filtering_is_enabled(cur_iface));

    if let Some(br_conf) = iface.bridge.as_ref()
        && (br_conf.options.is_some() || br_conf.vlan.is_some())
    {
        np_iface.bridge = Some(np_br);
    }
    Ok(vec![np_iface])
}

impl LinuxBridgeInterface {
    pub(crate) fn apply_linux_bridge_port_configs(
        &self,
        cur_br_iface: Option<&Self>,
    ) -> Vec<nispor::IfaceConf> {
        let mut ret: Vec<nispor::IfaceConf> = Vec::new();
        if let Some(ports_conf) =
            self.bridge.as_ref().and_then(|b| b.ports.as_ref())
        {
            for port_conf in ports_conf.iter().filter(|p| !p.is_name_only()) {
                let np_br_port_conf = gen_bridge_port_conf(
                    port_conf,
                    cur_br_iface.and_then(|cur_br_iface| {
                        cur_br_iface.get_port_conf(port_conf.name.as_str())
                    }),
                );
                let mut port_np_iface = nispor::IfaceConf::default();
                port_np_iface.name = port_conf.name.to_string();
                port_np_iface.bridge_port = Some(np_br_port_conf);
                ret.push(port_np_iface);
            }
        }
        ret
    }
}

fn gen_bridge_port_conf(
    port_conf: &LinuxBridgePortConfig,
    cur_port_conf: Option<&LinuxBridgePortConfig>,
) -> nispor::BridgePortConf {
    let mut ret = nispor::BridgePortConf::default();
    ret.hairpin_mode = port_conf.stp_hairpin_mode;
    ret.stp_priority = port_conf.stp_priority;
    ret.stp_path_cost = port_conf.stp_path_cost;
    ret.vlans = port_conf.vlan.as_ref().map(|vlan| {
        gen_bridge_vlan_conf(
            vlan,
            cur_port_conf.as_ref().and_then(|p| p.vlan.as_ref()),
        )
    });
    ret
}

fn parse_eth_mac(mac_str: &str) -> Result<[u8; 6], NipartError> {
    let mut mac_vec: Vec<u8> = Vec::new();
    for byte in mac_str.split(':') {
        mac_vec.push(u8::from_str_radix(byte, 16).map_err(|_| {
            NipartError::new(
                ErrorKind::InvalidArgument,
                format!(
                    "Invalid MAC address {mac_str}, expecting format like: \
                     02:69:4c:41:42:cd"
                ),
            )
        })?);
    }
    mac_vec.try_into().map_err(|_| {
        NipartError::new(
            ErrorKind::InvalidArgument,
            format!(
                "Invalid MAC address {mac_str}, expecting format like: \
                 02:69:4c:41:42:cd"
            ),
        )
    })
}
