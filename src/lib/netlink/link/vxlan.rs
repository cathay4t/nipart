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

pub(crate) async fn vxlan_link_builder(
    handle: &rtnetlink::Handle,
    iface: &crate::VxlanInterface,
    cur_iface: Option<&crate::VxlanInterface>,
) -> Result<
    rtnetlink::LinkMessageBuilder<rtnetlink::LinkVxlan>,
    crate::NipartError,
> {
    use rtnetlink::packet_route::link::InfoVxlan;

    let Some(vxlan_conf) = iface.vxlan.as_ref() else {
        return Err(crate::NipartError::new(
            crate::ErrorKind::InvalidArgument,
            format!(
                "Missing vxlan section for creating new VxLAN {}",
                iface.base.name
            ),
        ));
    };
    let mut builder = if let Some(id) = vxlan_conf.id {
        rtnetlink::LinkVxlan::new(iface.base.name.as_str(), id)
    } else if cur_iface.is_none() {
        return Err(crate::NipartError::new(
            crate::ErrorKind::InvalidArgument,
            format!(
                "Missing vxlan.id for creating new VxLAN {}",
                iface.base.name
            ),
        ));
    } else {
        rtnetlink::LinkMessageBuilder::<rtnetlink::LinkVxlan>::new(
            iface.base.name.as_str(),
        )
    };

    if let Some(parent) = vxlan_conf.base_iface.as_deref() {
        builder = builder.dev(super::get_iface_index(handle, parent).await?);
    }
    if let Some(remote) = vxlan_conf.remote.as_deref() {
        let remote = remote.parse::<std::net::IpAddr>().map_err(|e| {
            crate::NipartError::new(
                crate::ErrorKind::InvalidArgument,
                format!("Invalid vxlan remote address {remote}: {e}"),
            )
        })?;
        builder = match remote {
            std::net::IpAddr::V4(address) => builder.remote(address),
            std::net::IpAddr::V6(address) => builder.remote6(address),
        };
    }
    if let Some(local) = vxlan_conf.local.as_deref() {
        let local = local.parse::<std::net::IpAddr>().map_err(|e| {
            crate::NipartError::new(
                crate::ErrorKind::InvalidArgument,
                format!("Invalid vxlan local address {local}: {e}"),
            )
        })?;
        builder = match local {
            std::net::IpAddr::V4(address) => builder.local(address),
            std::net::IpAddr::V6(address) => builder.local6(address),
        };
    }
    if let Some(value) = vxlan_conf.destination_port {
        builder = builder.port(value);
    }
    if let Some(value) = vxlan_conf.learning {
        builder = builder.learning(value);
    }
    if let Some(value) = vxlan_conf.ttl {
        builder = builder.ttl(value);
    }
    if let Some(value) = vxlan_conf.tos {
        builder = builder.tos(value);
    }
    if let Some(value) = vxlan_conf.ageing {
        builder = builder.ageing(value);
    }
    if let Some(value) = vxlan_conf.max_address {
        builder = builder.limit(value);
    }
    if let (Some(min), Some(max)) =
        (vxlan_conf.src_port_min, vxlan_conf.src_port_max)
    {
        builder = builder.port_range(min, max);
    }
    if let Some(value) = vxlan_conf.proxy {
        builder = builder.proxy(value);
    }
    if let Some(value) = vxlan_conf.rsc {
        builder = builder.rsc(value);
    }
    if let Some(value) = vxlan_conf.l2miss {
        builder = builder.l2miss(value);
    }
    if let Some(value) = vxlan_conf.l3miss {
        builder = builder.l3miss(value);
    }
    if let Some(value) = vxlan_conf.udp_check_sum {
        builder = builder.udp_csum(value);
    }
    if let Some(value) = vxlan_conf.udp6_zero_check_sum_tx {
        builder = builder.append_info_data(InfoVxlan::UDPZeroCsumTX(value));
    }
    if let Some(value) = vxlan_conf.udp6_zero_check_sum_rx {
        builder = builder.append_info_data(InfoVxlan::UDPZeroCsumRX(value));
    }
    if let Some(value) = vxlan_conf.remote_check_sum_tx {
        builder = builder.append_info_data(InfoVxlan::RemCsumTX(value));
    }
    if let Some(value) = vxlan_conf.remote_check_sum_rx {
        builder = builder.append_info_data(InfoVxlan::RemCsumRX(value));
    }
    if vxlan_conf.gbp == Some(true) {
        builder = builder.append_info_data(InfoVxlan::Gbp);
    }
    if vxlan_conf.remote_check_sum_no_partial == Some(true) {
        builder = builder.append_info_data(InfoVxlan::RemCsumNoPartial);
    }
    if let Some(value) = vxlan_conf.collect_metadata {
        builder = builder.collect_metadata(value);
    }
    if let Some(value) = vxlan_conf.label {
        builder = builder.label(value);
    }
    if vxlan_conf.gpe == Some(true) {
        builder = builder.append_info_data(InfoVxlan::Gpe);
    }
    if vxlan_conf.ttl_inherit == Some(true) {
        builder = builder.append_info_data(InfoVxlan::TtlInheritFlag);
    }
    Ok(builder)
}
