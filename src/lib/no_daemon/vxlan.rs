// SPDX-License-Identifier: Apache-2.0

use crate::{BaseInterface, NipartError, VxlanConfig, VxlanInterface};

impl From<&nispor::VxlanInfo> for VxlanConfig {
    fn from(np_vxlan_conf: &nispor::VxlanInfo) -> Self {
        VxlanConfig {
            id: Some(np_vxlan_conf.vxlan_id),
            base_iface: Some(np_vxlan_conf.base_iface.clone()),
            remote: Some(np_vxlan_conf.remote.clone()),
            local: Some(np_vxlan_conf.local.clone()),
            learning: Some(np_vxlan_conf.learning),
            ttl: Some(np_vxlan_conf.ttl),
            tos: Some(np_vxlan_conf.tos),
            ageing: Some(np_vxlan_conf.ageing),
            max_address: Some(np_vxlan_conf.max_address),
            src_port_min: Some(np_vxlan_conf.src_port_min),
            src_port_max: Some(np_vxlan_conf.src_port_max),
            proxy: Some(np_vxlan_conf.proxy),
            rsc: Some(np_vxlan_conf.rsc),
            l2miss: Some(np_vxlan_conf.l2miss),
            l3miss: Some(np_vxlan_conf.l3miss),
            destination_port: Some(np_vxlan_conf.dst_port),
            udp_check_sum: Some(np_vxlan_conf.udp_check_sum),
            udp6_zero_check_sum_tx: Some(np_vxlan_conf.udp6_zero_check_sum_tx),
            udp6_zero_check_sum_rx: Some(np_vxlan_conf.udp6_zero_check_sum_rx),
            remote_check_sum_tx: Some(np_vxlan_conf.remote_check_sum_tx),
            remote_check_sum_rx: Some(np_vxlan_conf.remote_check_sum_rx),
            gbp: Some(np_vxlan_conf.gbp),
            remote_check_sum_no_partial: Some(
                np_vxlan_conf.remote_check_sum_no_partial,
            ),
            collect_metadata: Some(np_vxlan_conf.collect_metadata),
            label: Some(np_vxlan_conf.label),
            gpe: Some(np_vxlan_conf.gpe),
            ttl_inherit: Some(np_vxlan_conf.ttl_inherit),
        }
    }
}

impl From<&VxlanConfig> for nispor::VxlanConf {
    fn from(v: &VxlanConfig) -> Self {
        let mut np_vxlan = nispor::VxlanConf::default();
        np_vxlan.vxlan_id = v.id;
        np_vxlan.base_iface = v.base_iface.clone();
        if let Some(ref remote) = v.remote
            && let Ok(ip) = remote.parse::<std::net::IpAddr>()
        {
            np_vxlan.remote = Some(ip);
        }
        if let Some(ref local) = v.local
            && let Ok(ip) = local.parse::<std::net::IpAddr>()
        {
            np_vxlan.local = Some(ip);
        }
        np_vxlan.dst_port = v.destination_port;
        np_vxlan.learning = v.learning;
        np_vxlan.ttl = v.ttl;
        np_vxlan.tos = v.tos;
        np_vxlan.ageing = v.ageing;
        np_vxlan.max_address = v.max_address;
        np_vxlan.src_port_min = v.src_port_min;
        np_vxlan.src_port_max = v.src_port_max;
        np_vxlan.proxy = v.proxy;
        np_vxlan.rsc = v.rsc;
        np_vxlan.l2miss = v.l2miss;
        np_vxlan.l3miss = v.l3miss;
        np_vxlan.udp_check_sum = v.udp_check_sum;
        np_vxlan.udp6_zero_check_sum_tx = v.udp6_zero_check_sum_tx;
        np_vxlan.udp6_zero_check_sum_rx = v.udp6_zero_check_sum_rx;
        np_vxlan.remote_check_sum_tx = v.remote_check_sum_tx;
        np_vxlan.remote_check_sum_rx = v.remote_check_sum_rx;
        np_vxlan.gbp = v.gbp;
        np_vxlan.remote_check_sum_no_partial = v.remote_check_sum_no_partial;
        np_vxlan.collect_metadata = v.collect_metadata;
        np_vxlan.label = v.label;
        np_vxlan.gpe = v.gpe;
        np_vxlan.ttl_inherit = v.ttl_inherit;
        np_vxlan
    }
}

pub(crate) fn apply_vxlan_conf(
    mut np_iface: nispor::IfaceConf,
    iface: &VxlanInterface,
) -> Result<Vec<nispor::IfaceConf>, NipartError> {
    if let Some(vxlan_conf) = iface.vxlan.as_ref() {
        np_iface.vxlan = Some(vxlan_conf.into());
    }
    Ok(vec![np_iface])
}

impl VxlanInterface {
    pub(crate) fn new_from_nispor(
        base_iface: BaseInterface,
        np_iface: &nispor::Iface,
    ) -> Self {
        if let Some(np_vxlan_conf) = np_iface.vxlan.as_ref() {
            Self {
                base: base_iface,
                vxlan: Some(np_vxlan_conf.into()),
            }
        } else {
            Self {
                base: base_iface,
                ..Default::default()
            }
        }
    }
}
