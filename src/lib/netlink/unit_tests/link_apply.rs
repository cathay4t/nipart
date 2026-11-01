// SPDX-License-Identifier: Apache-2.0

use rtnetlink::packet_route::link::{
    InfoData, InfoPortData, LinkAttribute, LinkInfo, LinkMessage,
};

use crate::{
    BaseInterface, BridgeVlanConfig, InterfaceType, LinuxBridgeConfig,
    LinuxBridgeInterface, LinuxBridgeOptions, LinuxBridgePortConfig, VrfConfig,
    VrfInterface,
};

pub(super) fn bridge_iface(
    options: Option<LinuxBridgeOptions>,
    vlan: Option<BridgeVlanConfig>,
    ports: Option<Vec<LinuxBridgePortConfig>>,
) -> LinuxBridgeInterface {
    LinuxBridgeInterface {
        base: BaseInterface {
            name: "br0".to_string(),
            iface_type: InterfaceType::LinuxBridge,
            ..Default::default()
        },
        bridge: Some(LinuxBridgeConfig {
            options,
            ports,
            vlan,
        }),
    }
}

pub(super) fn vrf_iface(table_id: Option<u32>) -> VrfInterface {
    VrfInterface {
        base: BaseInterface {
            name: "vrf0".to_string(),
            iface_type: InterfaceType::Vrf,
            ..Default::default()
        },
        vrf: Some(VrfConfig {
            table_id,
            ports: None,
        }),
    }
}

pub(super) fn port_data(message: &LinkMessage) -> Option<InfoPortData> {
    message.attributes.iter().find_map(|attribute| {
        if let LinkAttribute::LinkInfo(infos) = attribute {
            infos.iter().find_map(|info| {
                if let LinkInfo::PortData(data) = info {
                    Some(data.clone())
                } else {
                    None
                }
            })
        } else {
            None
        }
    })
}

pub(super) fn info_data(message: &LinkMessage) -> Option<InfoData> {
    message.attributes.iter().find_map(|attribute| {
        if let LinkAttribute::LinkInfo(infos) = attribute {
            infos.iter().find_map(|info| {
                if let LinkInfo::Data(data) = info {
                    Some(data.clone())
                } else {
                    None
                }
            })
        } else {
            None
        }
    })
}
