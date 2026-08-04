// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nmstate project(https://nmstate.io/) which
// is under license of Apache 2.0, authors of original file are:
//  * Gris Ge <fge@redhat.com>
//  * Ales Musil <amusil@redhat.com>
//  * Enrique Llorente <ellorent@redhat.com>
//  * Íñigo Huguet <ihuguet@redhat.com>

use serde::{Deserialize, Serialize};

use crate::{
    BaseInterface, ErrorKind, InterfaceType, JsonDisplay, NipartError,
    NipartInterface,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonDisplay)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[non_exhaustive]
/// VLAN interface
pub struct VlanInterface {
    #[serde(flatten)]
    pub base: BaseInterface,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vlan: Option<VlanConfig>,
}

impl VlanInterface {
    pub fn new(name: String, vlan: VlanConfig) -> Self {
        Self {
            base: BaseInterface {
                name: name.to_string(),
                iface_type: InterfaceType::Vlan,
                ..Default::default()
            },
            vlan: Some(vlan),
        }
    }
}

impl Default for VlanInterface {
    fn default() -> Self {
        Self {
            base: BaseInterface {
                iface_type: InterfaceType::Vlan,
                ..Default::default()
            },
            vlan: None,
        }
    }
}

impl NipartInterface for VlanInterface {
    fn base_iface(&self) -> &BaseInterface {
        &self.base
    }

    fn base_iface_mut(&mut self) -> &mut BaseInterface {
        &mut self.base
    }

    fn is_virtual(&self) -> bool {
        true
    }

    fn parent(&self) -> Option<&str> {
        self.vlan.as_ref().and_then(|v| v.base_iface.as_deref())
    }

    /// * VLAN base-iface is mandatory for new VLAN
    /// * Always copy the base-iface and VLAN ID whenever vlan conf defined.
    /// * Sort and dedup QoS mapping
    fn sanitize(
        &self,
        current: Option<&Self>,
        for_save: &mut Self,
        for_apply: &mut Self,
        for_verify: &mut Self,
        merged: &mut Self,
    ) -> Result<(), NipartError> {
        let desired = self;

        if desired.is_absent() {
            return Ok(());
        }

        if desired.vlan.is_none() && current.is_none() {
            return Err(NipartError::new(
                ErrorKind::InvalidArgument,
                format!(
                    "`vlan.base-iface` and `vlan.id` are mandatory for \
                     creating new VLAN {}",
                    desired.name()
                ),
            ));
        }

        if current.is_none()
            && let Some(des_vlan_conf) = desired.vlan.as_ref()
        {
            if des_vlan_conf.base_iface.is_none() {
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "`vlan.base-iface` is mandatory for creating new VLAN \
                         {}",
                        desired.name()
                    ),
                ));
            }
            if des_vlan_conf.id.is_none() {
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "`vlan.id` is mandatory for creating new VLAN {}",
                        desired.name()
                    ),
                ));
            }
        }

        // Copy ID and base-iface if undefined in desired but VLAN section
        // included.

        if let Some(vlan_conf) = for_apply.vlan.as_mut()
            && let Some(cur_vlan_conf) =
                current.as_ref().and_then(|c| c.vlan.as_ref())
        {
            if vlan_conf.id.is_none() {
                vlan_conf.id = cur_vlan_conf.id;
            }
            if vlan_conf.base_iface.is_none() {
                vlan_conf.base_iface = cur_vlan_conf.base_iface.clone();
            }
            if let Some(for_save_conf) = for_save.vlan.as_mut() {
                for_save_conf.id = vlan_conf.id;
                for_save_conf.base_iface = vlan_conf.base_iface.clone();
            }
        }

        if let Some(vlan_conf) = merged.vlan.as_mut() {
            vlan_conf.sanitize();
        }
        if let Some(vlan_conf) = for_apply.vlan.as_mut() {
            vlan_conf.sanitize();
        }
        if let Some(vlan_conf) = for_save.vlan.as_mut() {
            vlan_conf.sanitize();
        }
        if let Some(vlan_conf) = for_verify.vlan.as_mut() {
            vlan_conf.sanitize();
        }

        Ok(())
    }

    /// Include both base-iface and ID if changed.
    fn include_diff_context(&mut self, desired: &Self, current: &Self) {
        let diff = self;
        if let Some(des_vlan_conf) = desired.vlan.as_ref()
            && let Some(cur_vlan_conf) = current.vlan.as_ref()
            && des_vlan_conf != cur_vlan_conf
        {
            let mut diff_vlan_conf = des_vlan_conf.clone();
            if diff_vlan_conf.base_iface.is_none() {
                diff_vlan_conf.base_iface = cur_vlan_conf.base_iface.clone();
            }
            if diff_vlan_conf.id.is_none() {
                diff_vlan_conf.id = cur_vlan_conf.id;
            }
            diff.vlan = Some(diff_vlan_conf);
        }
    }

    fn sanitize_before_verify(&mut self, current: &mut Self) {
        if let Some(vlan_conf) = current.vlan.as_mut() {
            if vlan_conf.ingress_qos_map.is_none() {
                vlan_conf.ingress_qos_map = Some(Vec::new());
            }
            if vlan_conf.egress_qos_map.is_none() {
                vlan_conf.egress_qos_map = Some(Vec::new());
            }
        }
    }

    fn need_delete_before_change(&self, current: &Self) -> bool {
        let desired = self;
        if let Some(des) = desired.vlan.as_ref()
            && let Some(cur) = current.vlan.as_ref()
        {
            if des.id.is_some() && des.id != cur.id {
                return true;
            }
            if des.base_iface.is_some() && des.base_iface != cur.base_iface {
                return true;
            }
            if des.protocol.is_some() && des.protocol != cur.protocol {
                return true;
            }
            // Kernel IFLA_VLAN_INGRESS_QOS/EGRESS_QOS is add-only, cannot
            // clear existing entries. Delete and recreate the VLAN instead.
            if des.ingress_qos_map == Some(vec![])
                && cur.ingress_qos_map.as_ref().is_some_and(|v| !v.is_empty())
            {
                return true;
            }
            if des.egress_qos_map == Some(vec![])
                && cur.egress_qos_map.as_ref().is_some_and(|v| !v.is_empty())
            {
                return true;
            }
        }
        false
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, JsonDisplay,
)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[non_exhaustive]
pub struct VlanConfig {
    /// Change existing VLAN base interface will cause VLAN been deleted first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_iface: Option<String>,
    /// Change existing VLAN ID will cause VLAN been deleted first.
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::deserializer::option_u16_or_string"
    )]
    pub id: Option<u16>,
    /// Could be `802.1q` or `802.1ad`. Default to `802.1q` if not defined.
    /// Change existing VLAN protocol will cause VLAN been deleted first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<VlanProtocol>,
    /// Could be `gvrp`, `mvrp` or `none`. Default to none if not defined.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_protocol: Option<VlanRegistrationProtocol>,
    /// reordering of output packet headers. Default to True if not defined.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub reorder_headers: Option<bool>,
    /// loose binding of the interface to its master device's operating state
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub loose_binding: Option<bool>,
    /// VLAN device link state tracks the state of bridge ports that are
    /// members of the VLAN
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub bridge_binding: Option<bool>,
    /// Mapping VLAN header priority to linux internal packet priority for
    /// incoming packet.
    /// The maximum of VLAN priority is 7 according to
    /// 802.1Q-2018 PCP field definition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ingress_qos_map: Option<Vec<VlanQosMapping>>,
    /// Mapping linux internal packet priority to VLAN header priority for
    /// outgoing packet.
    /// The maximum of VLAN priority is 7 according to
    /// 802.1Q-2018 PCP field definition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub egress_qos_map: Option<Vec<VlanQosMapping>>,
}

impl VlanConfig {
    fn sanitize(&mut self) {
        if let Some(qos_map) = self.ingress_qos_map.as_mut() {
            qos_map.sort_unstable();
            qos_map.dedup();
        }
        if let Some(qos_map) = self.egress_qos_map.as_mut() {
            qos_map.sort_unstable();
            qos_map.dedup();
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default,
)]
pub enum VlanProtocol {
    #[serde(rename = "802.1q")]
    /// Deserialize and serialize from/to `802.1q`.
    #[default]
    Ieee8021Q,
    #[serde(rename = "802.1ad")]
    /// Deserialize and serialize from/to `802.1ad`.
    Ieee8021Ad,
    /// Unknown VLAN protocol
    Unknown,
}

impl std::fmt::Display for VlanProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Ieee8021Q => "802.1q",
                Self::Ieee8021Ad => "802.1ad",
                Self::Unknown => "unknown",
            }
        )
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonDisplay,
)]
#[serde(rename_all = "kebab-case")]
pub enum VlanRegistrationProtocol {
    /// GARP VLAN Registration Protocol
    Gvrp,
    /// Multiple VLAN Registration Protocol
    Mvrp,
    /// No Registration Protocol
    None,
}

/// VLAN QoS Mapping
/// Mapping between linux internal packet priority and VLAN header priority for
/// incoming or outgoing packet.
#[derive(
    Serialize,
    Deserialize,
    Debug,
    PartialEq,
    Eq,
    Clone,
    Copy,
    Default,
    PartialOrd,
    Ord,
)]
pub struct VlanQosMapping {
    #[serde(deserialize_with = "crate::deserializer::u32_or_string")]
    pub from: u32,
    #[serde(deserialize_with = "crate::deserializer::u32_or_string")]
    pub to: u32,
}

impl std::fmt::Display for VlanQosMapping {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.from, self.to)
    }
}
