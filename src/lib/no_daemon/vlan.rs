// SPDX-License-Identifier: Apache-2.0

use crate::{
    ErrorKind, NipartError, VlanConfig, VlanInterface, VlanProtocol,
    VlanQosMapping, VlanRegistrationProtocol,
};

impl From<VlanProtocol> for nispor::VlanProtocol {
    fn from(v: VlanProtocol) -> Self {
        match v {
            VlanProtocol::Ieee8021Q => Self::Ieee8021Q,
            VlanProtocol::Ieee8021Ad => Self::Ieee8021AD,
            VlanProtocol::Unknown => {
                log::debug!("Unknown VLAN protocol {v}, treating as 802.1q");
                Self::Ieee8021Q
            }
        }
    }
}

impl From<&VlanQosMapping> for nispor::VlanQosMapping {
    fn from(v: &VlanQosMapping) -> Self {
        Self {
            from: v.from,
            to: v.to,
        }
    }
}

impl From<&VlanConfig> for nispor::VlanConf {
    fn from(v: &VlanConfig) -> Self {
        let mut np_vlan = nispor::VlanConf::default();
        np_vlan.vlan_id = v.id;
        np_vlan.base_iface = v.base_iface.clone();
        np_vlan.protocol = v.protocol.map(|v| v.into());
        if let Some(protocol) = v.registration_protocol {
            match protocol {
                VlanRegistrationProtocol::Gvrp => {
                    np_vlan.is_gvrp = Some(true);
                    np_vlan.is_mvrp = Some(false);
                }
                VlanRegistrationProtocol::Mvrp => {
                    np_vlan.is_gvrp = Some(false);
                    np_vlan.is_mvrp = Some(true);
                }
                VlanRegistrationProtocol::None => {
                    np_vlan.is_gvrp = Some(false);
                    np_vlan.is_mvrp = Some(false);
                }
            }
        }

        if let Some(v) = v.reorder_headers {
            np_vlan.is_reorder_hdr = Some(v)
        }
        if let Some(v) = v.loose_binding {
            np_vlan.is_loose_binding = Some(v)
        }
        if let Some(v) = v.bridge_binding {
            np_vlan.is_bridge_binding = Some(v)
        }
        if let Some(qoses) = v.ingress_qos_map.as_ref() {
            np_vlan.ingress_qos_map =
                Some(qoses.iter().map(nispor::VlanQosMapping::from).collect());
        }
        if let Some(qoses) = v.egress_qos_map.as_ref() {
            np_vlan.egress_qos_map =
                Some(qoses.iter().map(nispor::VlanQosMapping::from).collect());
        }
        np_vlan
    }
}

pub(crate) fn apply_vlan_conf(
    mut np_iface: nispor::IfaceConf,
    iface: &VlanInterface,
) -> Result<Vec<nispor::IfaceConf>, NipartError> {
    if let Some(vlan_conf) = iface.vlan.as_ref() {
        if vlan_conf.id.is_some() && vlan_conf.base_iface.as_ref().is_some() {
            np_iface.vlan = Some(vlan_conf.into());
        } else {
            return Err(NipartError::new(
                ErrorKind::Bug,
                format!(
                    "apply_vlan_conf() got VLAN without ID or base-iface: \
                     {iface:?}"
                ),
            ));
        }
    }
    Ok(vec![np_iface])
}
