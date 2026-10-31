// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nispor project
// (https://github.com/nispor/nispor) which is under license of Apache 2.0.

use rtnetlink::packet_route::link::{InfoData, InfoVxlan};

use crate::VxlanConfig;

pub(crate) fn parse_vxlan(data: &InfoData) -> Option<VxlanConfig> {
    let InfoData::Vxlan(infos) = data else {
        return None;
    };
    let mut config = VxlanConfig {
        id: Some(0),
        base_iface: Some(String::new()),
        remote: Some(String::new()),
        local: Some(String::new()),
        destination_port: Some(0),
        learning: Some(false),
        ttl: Some(0),
        tos: Some(0),
        ageing: Some(0),
        max_address: Some(0),
        src_port_min: Some(0),
        src_port_max: Some(0),
        proxy: Some(false),
        rsc: Some(false),
        l2miss: Some(false),
        l3miss: Some(false),
        udp_check_sum: Some(false),
        udp6_zero_check_sum_tx: Some(false),
        udp6_zero_check_sum_rx: Some(false),
        remote_check_sum_tx: Some(false),
        remote_check_sum_rx: Some(false),
        gbp: Some(false),
        remote_check_sum_no_partial: Some(false),
        collect_metadata: Some(false),
        label: Some(0),
        gpe: Some(false),
        ttl_inherit: Some(false),
        ..Default::default()
    };
    for info in infos {
        match info {
            InfoVxlan::Id(id) => config.id = Some(*id),
            InfoVxlan::Group(address) => {
                config.remote = Some(address.to_string())
            }
            InfoVxlan::Group6(address) => {
                config.remote = Some(address.to_string())
            }
            InfoVxlan::Link(index) => {
                config.base_iface = Some(index.to_string());
            }
            InfoVxlan::Local(address) => {
                config.local = Some(address.to_string())
            }
            InfoVxlan::Local6(address) => {
                config.local = Some(address.to_string())
            }
            InfoVxlan::Tos(tos) => config.tos = Some(*tos),
            InfoVxlan::Ttl(ttl) => config.ttl = Some(*ttl),
            InfoVxlan::Label(label) => config.label = Some(*label),
            InfoVxlan::Learning(learning) => config.learning = Some(*learning),
            InfoVxlan::Ageing(ageing) => config.ageing = Some(*ageing),
            InfoVxlan::Limit(limit) => config.max_address = Some(*limit),
            InfoVxlan::PortRange((min, max)) => {
                config.src_port_min = Some(*min);
                config.src_port_max = Some(*max);
            }
            InfoVxlan::Proxy(proxy) => config.proxy = Some(*proxy),
            InfoVxlan::Rsc(rsc) => config.rsc = Some(*rsc),
            InfoVxlan::L2Miss(l2miss) => config.l2miss = Some(*l2miss),
            InfoVxlan::L3Miss(l3miss) => config.l3miss = Some(*l3miss),
            InfoVxlan::Port(port) => config.destination_port = Some(*port),
            InfoVxlan::UDPCsum(csum) => config.udp_check_sum = Some(*csum),
            InfoVxlan::UDPZeroCsumTX(csum) => {
                config.udp6_zero_check_sum_tx = Some(*csum)
            }
            InfoVxlan::UDPZeroCsumRX(csum) => {
                config.udp6_zero_check_sum_rx = Some(*csum)
            }
            InfoVxlan::RemCsumTX(csum) => {
                config.remote_check_sum_tx = Some(*csum)
            }
            InfoVxlan::RemCsumRX(csum) => {
                config.remote_check_sum_rx = Some(*csum)
            }
            InfoVxlan::RemCsumNoPartial => {
                config.remote_check_sum_no_partial = Some(true)
            }
            InfoVxlan::Gbp => config.gbp = Some(true),
            InfoVxlan::Gpe => config.gpe = Some(true),
            InfoVxlan::TtlInherit(ttl_inherit) => {
                config.ttl_inherit = Some(*ttl_inherit != 0)
            }
            InfoVxlan::CollectMetadata(collect) => {
                config.collect_metadata = Some(*collect)
            }
            _ => (),
        }
    }
    Some(config)
}
