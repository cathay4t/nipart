// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::{
    BaseInterface, ErrorKind, Interface, InterfaceType, JsonDisplay,
    MergedInterface, NipartError, NipartInterface,
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
    const LIVE_CHANGE_PROP_LIST: &'static [&'static str] = &[
        "remote", "local", "learning", "ttl", "tos", "ageing", "label",
    ];

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

    fn config_value(&self) -> serde_json::Value {
        match self.vxlan.as_ref() {
            Some(c) => serde_json::to_value(c).unwrap_or_default(),
            None => serde_json::Value::Null,
        }
    }

    fn sanitize_iface_specfic(
        &mut self,
        current: Option<&Self>,
    ) -> Result<(), NipartError> {
        if current.is_none() && self.vxlan.is_none() && !self.is_absent() {
            return Err(NipartError::new(
                ErrorKind::InvalidArgument,
                format!(
                    "`vxlan` configuration is mandatory for creating new \
                     VxLAN {}",
                    self.name()
                ),
            ));
        }
        if let Some(vxlan_conf) = self.vxlan.as_mut() {
            if let Some(cur_vxlan_conf) =
                current.as_ref().and_then(|c| c.vxlan.as_ref())
            {
                if vxlan_conf.id.is_none() {
                    vxlan_conf.id = cur_vxlan_conf.id;
                }
                if vxlan_conf.base_iface.is_none() {
                    vxlan_conf.base_iface = cur_vxlan_conf.base_iface.clone();
                }
            } else {
                if vxlan_conf.base_iface.is_none() {
                    return Err(NipartError::new(
                        ErrorKind::InvalidArgument,
                        format!(
                            "`vxlan.base-iface` is mandatory for creating new \
                             VxLAN {}",
                            self.name()
                        ),
                    ));
                }
                if vxlan_conf.id.is_none() {
                    return Err(NipartError::new(
                        ErrorKind::InvalidArgument,
                        format!(
                            "`vxlan.id` is mandatory for creating new VxLAN {}",
                            self.name()
                        ),
                    ));
                }
            }
            if let Some(id) = vxlan_conf.id
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
            if let Some(ref remote) = vxlan_conf.remote
                && !remote.is_empty()
                && remote.parse::<std::net::IpAddr>().is_err()
            {
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "`vxlan.remote` '{remote}' is not a valid IP address"
                    ),
                ));
            }
            if let Some(ref local) = vxlan_conf.local
                && !local.is_empty()
                && local.parse::<std::net::IpAddr>().is_err()
            {
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "`vxlan.local` '{local}' is not a valid IP address"
                    ),
                ));
            }
            if (vxlan_conf.src_port_min.is_some()
                || vxlan_conf.src_port_max.is_some())
                && (vxlan_conf.src_port_min.is_none()
                    || vxlan_conf.src_port_max.is_none())
            {
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    "`vxlan.src-port-min` and `vxlan.src-port-max` must be \
                     set together"
                        .to_string(),
                ));
            }
        }

        Ok(())
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

impl MergedInterface {
    pub(crate) fn post_merge_sanitize_vxlan(&mut self) {
        if let (
            Some(Interface::Vxlan(apply_iface)),
            Some(Interface::Vxlan(verify_iface)),
            Some(Interface::Vxlan(cur_iface)),
        ) = (&mut self.for_apply, &mut self.for_verify, &self.current)
        {
            if let Some(apply_vxlan) = &mut apply_iface.vxlan
                && apply_vxlan.base_iface.is_none()
            {
                apply_vxlan.base_iface = cur_iface
                    .vxlan
                    .as_ref()
                    .and_then(|vxlan| vxlan.base_iface.clone());
            }
            if let Some(verify_vxlan) = &mut verify_iface.vxlan
                && verify_vxlan.base_iface.is_none()
            {
                verify_vxlan.base_iface = cur_iface
                    .vxlan
                    .as_ref()
                    .and_then(|vxlan| vxlan.base_iface.clone());
            }
        }
    }
}
