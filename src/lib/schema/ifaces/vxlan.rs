// SPDX-License-Identifier: Apache-2.0

use std::{net::IpAddr, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{
    BaseInterface, ErrorKind, InterfaceType, JsonDisplay, NipartError,
    NipartInterface,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonDisplay)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[non_exhaustive]
/// VxLAN interface
pub struct VxlanInterface {
    #[serde(flatten)]
    pub base: BaseInterface,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vxlan: Option<VxlanConfig>,
}

impl VxlanInterface {
    pub fn new(name: String, vxlan: VxlanConfig) -> Self {
        Self {
            base: BaseInterface {
                name: name.to_string(),
                iface_type: InterfaceType::Vxlan,
                ..Default::default()
            },
            vxlan: Some(vxlan),
        }
    }
}

impl Default for VxlanInterface {
    fn default() -> Self {
        Self {
            base: BaseInterface {
                iface_type: InterfaceType::Vxlan,
                ..Default::default()
            },
            vxlan: None,
        }
    }
}

impl NipartInterface for VxlanInterface {
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
        self.vxlan.as_ref().and_then(|v| v.base_iface.as_deref())
    }

    fn sanitize(
        &self,
        current: Option<&Self>,
        _for_save: &mut Self,
        for_apply: &mut Self,
        for_verify: &mut Self,
        _merged: &mut Self,
    ) -> Result<(), NipartError> {
        let desired = self;
        if desired.is_absent() {
            return Ok(());
        }

        if let Some(vxlan_conf) = for_apply.vxlan.as_mut()
            && let Err(e) = vxlan_conf.sanitize()
        {
            return Err(NipartError::new(
                ErrorKind::InvalidArgument,
                format!(
                    "Invalid VxLAN config for interface {}/{}: {e}",
                    desired.name(),
                    desired.iface_type()
                ),
            ));
        }

        if current.is_none() {
            if desired.vxlan.is_none() {
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "`vxlan` configuration is mandatory for creating new \
                         VxLAN {}",
                        desired.name()
                    ),
                ));
            }
            if desired.vxlan.as_ref().and_then(|c| c.id.as_ref()).is_none() {
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "`vxlan.id` is mandatory for creating new VxLAN {}",
                        desired.name()
                    ),
                ));
            }
        }

        if let Some(apply_vxlan) = for_apply.vxlan.as_mut()
            && apply_vxlan.base_iface.is_none()
            && let Some(cur_iface) = current.as_ref()
        {
            apply_vxlan.base_iface = cur_iface
                .vxlan
                .as_ref()
                .and_then(|vxlan| vxlan.base_iface.clone());
        }
        if let Some(verify_vxlan) = for_verify.vxlan.as_mut()
            && verify_vxlan.base_iface.is_none()
            && let Some(cur_iface) = current.as_ref()
        {
            verify_vxlan.base_iface = cur_iface
                .vxlan
                .as_ref()
                .and_then(|vxlan| vxlan.base_iface.clone());
        }
        Ok(())
    }

    fn need_delete_before_change(&self, current: &Self) -> bool {
        let desired = self;
        // TODO: Use gen_diff(), then compare with allow list.
        if let Some(des) = desired.vxlan.as_ref()
            && let Some(cur) = current.vxlan.as_ref()
        {
            if des.id.is_some() && des.id != cur.id {
                return true;
            }
            if des.base_iface.is_some() && des.base_iface != cur.base_iface {
                return true;
            }
            if des.destination_port.is_some()
                && des.destination_port != cur.destination_port
            {
                return true;
            }
            if des.max_address.is_some() && des.max_address != cur.max_address {
                return true;
            }
            if des.src_port_min.is_some()
                && des.src_port_min != cur.src_port_min
            {
                return true;
            }
            if des.src_port_max.is_some()
                && des.src_port_max != cur.src_port_max
            {
                return true;
            }
            if des.proxy.is_some() && des.proxy != cur.proxy {
                return true;
            }
            if des.rsc.is_some() && des.rsc != cur.rsc {
                return true;
            }
            if des.l2miss.is_some() && des.l2miss != cur.l2miss {
                return true;
            }
            if des.l3miss.is_some() && des.l3miss != cur.l3miss {
                return true;
            }
            if des.udp_check_sum.is_some()
                && des.udp_check_sum != cur.udp_check_sum
            {
                return true;
            }
            if des.udp6_zero_check_sum_tx.is_some()
                && des.udp6_zero_check_sum_tx != cur.udp6_zero_check_sum_tx
            {
                return true;
            }
            if des.udp6_zero_check_sum_rx.is_some()
                && des.udp6_zero_check_sum_rx != cur.udp6_zero_check_sum_rx
            {
                return true;
            }
            if des.remote_check_sum_tx.is_some()
                && des.remote_check_sum_tx != cur.remote_check_sum_tx
            {
                return true;
            }
            if des.remote_check_sum_rx.is_some()
                && des.remote_check_sum_rx != cur.remote_check_sum_rx
            {
                return true;
            }
            if des.gbp.is_some() && des.gbp != cur.gbp {
                return true;
            }
            if des.remote_check_sum_no_partial.is_some()
                && des.remote_check_sum_no_partial
                    != cur.remote_check_sum_no_partial
            {
                return true;
            }
            if des.collect_metadata.is_some()
                && des.collect_metadata != cur.collect_metadata
            {
                return true;
            }
            if des.gpe.is_some() && des.gpe != cur.gpe {
                return true;
            }
            if des.ttl_inherit.is_some() && des.ttl_inherit != cur.ttl_inherit {
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
pub struct VxlanConfig {
    /// VNI (VxLAN Network Identifier)
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::deserializer::option_u32_or_string"
    )]
    pub id: Option<u32>,
    /// Device name of the base interface
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_iface: Option<String>,
    /// Unicast or multicast IP address of the remote VXLAN tunnel endpoint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    /// IP address of the local VXLAN tunnel endpoint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local: Option<String>,
    /// Destination port. Default to 4789 if not defined.
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::deserializer::option_u16_or_string"
    )]
    pub destination_port: Option<u16>,
    /// whether to enable VXLAN learning or not. Default to true.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub learning: Option<bool>,
    /// TTL of the VXLAN tunnel protocol
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::deserializer::option_u8_or_string"
    )]
    pub ttl: Option<u8>,
    /// TOS of the VXLAN tunnel protocol
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::deserializer::option_u8_or_string"
    )]
    pub tos: Option<u8>,
    /// Lifetime in seconds of FDB entry learnt by the kernel
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::deserializer::option_u32_or_string"
    )]
    pub ageing: Option<u32>,
    /// Maximum number of FDB entries
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::deserializer::option_u32_or_string"
    )]
    pub max_address: Option<u32>,
    /// Source port range min
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::deserializer::option_u16_or_string"
    )]
    pub src_port_min: Option<u16>,
    /// Source port range max
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::deserializer::option_u16_or_string"
    )]
    pub src_port_max: Option<u16>,
    /// ARP proxy
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub proxy: Option<bool>,
    /// Route short circuit
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub rsc: Option<bool>,
    /// Netlink LLADDR miss notification
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub l2miss: Option<bool>,
    /// Netlink IP ADDR miss notification
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub l3miss: Option<bool>,
    /// UDP checksum
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub udp_check_sum: Option<bool>,
    /// Allow sending UDP zero checksum for IPv6
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub udp6_zero_check_sum_tx: Option<bool>,
    /// Allow receiving UDP zero checksum for IPv6
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub udp6_zero_check_sum_rx: Option<bool>,
    /// Remote checksum transmission
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub remote_check_sum_tx: Option<bool>,
    /// Remote checksum reception
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub remote_check_sum_rx: Option<bool>,
    /// Group Based Policy
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub gbp: Option<bool>,
    /// Remote checksum no partial
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub remote_check_sum_no_partial: Option<bool>,
    /// Collect metadata
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub collect_metadata: Option<bool>,
    /// Label (for IPv6 only)
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::deserializer::option_u32_or_string"
    )]
    pub label: Option<u32>,
    /// Generic Protocol Extension
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub gpe: Option<bool>,
    /// TTL inherit
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub ttl_inherit: Option<bool>,
}

impl VxlanConfig {
    fn sanitize(&self) -> Result<(), NipartError> {
        if let Some(id) = self.id
            && id > 16777215
        {
            return Err(NipartError::new(
                ErrorKind::InvalidArgument,
                format!(
                    "`vxlan.id` {} is out of range: VNI must be 0-16777215",
                    id
                ),
            ));
        }
        if let Some(remote) = self.remote.as_ref()
            && !remote.is_empty()
            && let Err(_e) = IpAddr::from_str(remote)
        {
            return Err(NipartError::new(
                ErrorKind::InvalidArgument,
                format!("`vxlan.remote` {remote} is not valid IP address"),
            ));
        }

        if let Some(local) = self.local.as_ref()
            && !local.is_empty()
            && let Err(_e) = IpAddr::from_str(local)
        {
            return Err(NipartError::new(
                ErrorKind::InvalidArgument,
                format!("`vxlan.local` {local} is not valid IP address"),
            ));
        }
        if (self.src_port_min.is_some() || self.src_port_max.is_some())
            && (self.src_port_min.is_none() || self.src_port_max.is_none())
        {
            return Err(NipartError::new(
                ErrorKind::InvalidArgument,
                "`vxlan.src-port-min` and `vxlan.src-port-max` must be set \
                 together"
                    .to_string(),
            ));
        }

        Ok(())
    }
}
