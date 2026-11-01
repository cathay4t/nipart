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

