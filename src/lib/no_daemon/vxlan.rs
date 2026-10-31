// SPDX-License-Identifier: Apache-2.0

use crate::{NipartError, VxlanConfig, VxlanInterface};

impl From<&VxlanConfig> for nispor::VxlanConf {
    fn from(v: &VxlanConfig) -> Self {
        let mut np_vxlan = nispor::VxlanConf::default();
        np_vxlan.vxlan_id = v.id;
        np_vxlan.base_iface = v.base_iface.clone();
        if let Some(remote) = v.remote.as_ref()
            && let Ok(ip) = remote.parse::<std::net::IpAddr>()
        {
            np_vxlan.remote = Some(ip);
        }
        if let Some(local) = v.local.as_ref()
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
