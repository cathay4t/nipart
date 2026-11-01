// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nispor project
// (https://github.com/nispor/nispor) which is under license of Apache 2.0.

use rtnetlink::packet_route::link::{
    InfoData, InfoVlan, VlanFlags, VlanProtocol as NlVlanProtocol,
    VlanQosMapping as NlVlanQosMapping,
};

use crate::{
    VlanConfig, VlanProtocol, VlanQosMapping, VlanRegistrationProtocol,
};

pub(crate) fn parse_vlan(data: &InfoData) -> Option<VlanConfig> {
    let InfoData::Vlan(infos) = data else {
        return None;
    };
    let mut config = VlanConfig {
        base_iface: Some(String::new()),
        id: Some(0),
        protocol: Some(VlanProtocol::Ieee8021Q),
        registration_protocol: Some(VlanRegistrationProtocol::None),
        reorder_headers: Some(false),
        loose_binding: Some(false),
        bridge_binding: Some(false),
        ..Default::default()
    };
    for info in infos {
        match info {
            InfoVlan::Id(id) => config.id = Some(*id),
            InfoVlan::Protocol(protocol) => {
                config.protocol = Some(vlan_protocol_from_nl80211(*protocol));
            }
            InfoVlan::Flags((flags, _)) => {
                config.reorder_headers =
                    Some(flags.contains(VlanFlags::ReorderHdr));
                config.loose_binding =
                    Some(flags.contains(VlanFlags::LooseBinding));
                config.bridge_binding =
                    Some(flags.contains(VlanFlags::BridgeBinding));
                config.registration_protocol =
                    if flags.contains(VlanFlags::Gvrp) {
                        Some(VlanRegistrationProtocol::Gvrp)
                    } else if flags.contains(VlanFlags::Mvrp) {
                        Some(VlanRegistrationProtocol::Mvrp)
                    } else {
                        Some(VlanRegistrationProtocol::None)
                    };
            }
            InfoVlan::IngressQos(maps) => {
                config.ingress_qos_map = qos_map_from_nl80211(maps);
            }
            InfoVlan::EgressQos(maps) => {
                config.egress_qos_map = qos_map_from_nl80211(maps);
            }
            _ => (),
        }
    }
    Some(config)
}

fn vlan_protocol_from_nl80211(protocol: NlVlanProtocol) -> VlanProtocol {
    match protocol {
        NlVlanProtocol::Ieee8021Q => VlanProtocol::Ieee8021Q,
        NlVlanProtocol::Ieee8021Ad => VlanProtocol::Ieee8021Ad,
        _ => VlanProtocol::Unknown,
    }
}

fn qos_map_from_nl80211(
    mappings: &[NlVlanQosMapping],
) -> Option<Vec<VlanQosMapping>> {
    let mut ret: Vec<VlanQosMapping> = mappings
        .iter()
        .filter_map(|mapping| {
            if let NlVlanQosMapping::Mapping(from, to) = mapping {
                Some(VlanQosMapping {
                    from: *from,
                    to: *to,
                })
            } else {
                None
            }
        })
        .collect();
    if ret.is_empty() {
        None
    } else {
        ret.sort_unstable();
        Some(ret)
    }
}

impl From<VlanProtocol> for NlVlanProtocol {
    fn from(value: VlanProtocol) -> Self {
        match value {
            VlanProtocol::Ieee8021Q | VlanProtocol::Unknown => Self::Ieee8021Q,
            VlanProtocol::Ieee8021Ad => Self::Ieee8021Ad,
        }
    }
}

pub(crate) async fn vlan_link_builder(
    handle: &rtnetlink::Handle,
    iface: &crate::VlanInterface,
    cur_iface: Option<&crate::VlanInterface>,
) -> Result<
    rtnetlink::LinkMessageBuilder<rtnetlink::LinkVlan>,
    crate::NipartError,
> {
    use rtnetlink::packet_route::link::{InfoKind, VlanFlags};

    let mut builder =
        rtnetlink::LinkMessageBuilder::<rtnetlink::LinkVlan>::new_with_info_kind(
            InfoKind::Vlan,
        )
        .name(iface.base.name.to_string());
    let Some(vlan_conf) = iface.vlan.as_ref() else {
        return Ok(builder);
    };
    if cur_iface.is_none()
        && (vlan_conf.id.is_none() || vlan_conf.base_iface.is_none())
    {
        return Err(crate::NipartError::new(
            crate::ErrorKind::InvalidArgument,
            format!(
                "Need VLAN ID and base interface for creating new VLAN {}",
                iface.base.name
            ),
        ));
    }
    if let Some(parent) = vlan_conf.base_iface.as_deref() {
        let parent_index = super::get_iface_index(handle, parent).await?;
        builder = builder.link(parent_index);
    }
    if let Some(id) = vlan_conf.id {
        builder = builder.id(id);
    }
    if let Some(protocol) = vlan_conf.protocol {
        builder = builder.protocol(protocol.into());
    }

    let mut flags = VlanFlags::empty();
    let mut flags_mask = VlanFlags::empty();
    let mut set_flag = |value: Option<bool>, flag: VlanFlags| {
        if let Some(value) = value {
            flags_mask |= flag;
            if value {
                flags |= flag;
            }
        }
    };
    set_flag(vlan_conf.reorder_headers, VlanFlags::ReorderHdr);
    set_flag(vlan_conf.loose_binding, VlanFlags::LooseBinding);
    set_flag(vlan_conf.bridge_binding, VlanFlags::BridgeBinding);
    match vlan_conf.registration_protocol {
        Some(crate::VlanRegistrationProtocol::Gvrp) => {
            set_flag(Some(true), VlanFlags::Gvrp);
            set_flag(Some(false), VlanFlags::Mvrp);
        }
        Some(crate::VlanRegistrationProtocol::Mvrp) => {
            set_flag(Some(false), VlanFlags::Gvrp);
            set_flag(Some(true), VlanFlags::Mvrp);
        }
        Some(crate::VlanRegistrationProtocol::None) => {
            set_flag(Some(false), VlanFlags::Gvrp);
            set_flag(Some(false), VlanFlags::Mvrp);
        }
        None => (),
    }
    if flags_mask != VlanFlags::empty() {
        builder = builder.flags(flags, flags_mask);
    }

    let cur_vlan_conf = cur_iface.and_then(|i| i.vlan.as_ref());
    let ingress = effective_qos_map(
        vlan_conf.ingress_qos_map.as_ref(),
        cur_vlan_conf.and_then(|c| c.ingress_qos_map.as_ref()),
    );
    let egress = effective_qos_map(
        vlan_conf.egress_qos_map.as_ref(),
        cur_vlan_conf.and_then(|c| c.egress_qos_map.as_ref()),
    );
    if !ingress.is_empty() || !egress.is_empty() {
        builder = builder.qos(
            ingress.iter().map(|entry| rtnetlink::QosMapping {
                from: entry.from,
                to: entry.to,
            }),
            egress.iter().map(|entry| rtnetlink::QosMapping {
                from: entry.from,
                to: entry.to,
            }),
        );
    }
    Ok(builder)
}

fn effective_qos_map(
    desired: Option<&Vec<crate::VlanQosMapping>>,
    current: Option<&Vec<crate::VlanQosMapping>>,
) -> Vec<crate::VlanQosMapping> {
    let Some(desired) = desired else {
        return Vec::new();
    };
    let mut result = desired.clone();
    if let Some(current) = current {
        for current_entry in current {
            if !desired.iter().any(|entry| entry.from == current_entry.from) {
                result.push(crate::VlanQosMapping {
                    from: current_entry.from,
                    to: 0,
                });
            }
        }
    }
    result
}
