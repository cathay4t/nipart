// SPDX-License-Identifier: Apache-2.0

use serde::{
    Deserialize, Deserializer, Serialize, Serializer, ser::SerializeMap,
};

use crate::{
    Interface, InterfaceIdentifier, InterfaceLinkEvent, Interfaces,
    JsonDisplay, NipartInterface,
};

/// Defines whether and when an interface should be activated automatically.
#[derive(Debug, Clone, PartialEq, Eq, Default, JsonDisplay)]
#[non_exhaustive]
pub enum InterfaceAutoConnect {
    // TODO: Support delay for up/down action to prevent flipping.
    /// Activate the interface automatically.
    /// For physical interface, the config is applied upon carrier up.
    /// For virtual interface(e.g. bond, VLAN, linux bridge), the config is
    /// applied upon boot or apply action.
    /// Serialize to boolean `true`.
    #[default]
    AutoConnect,
    /// Only apply the config upon apply action, ignored in boot action.
    /// Serialize to boolean `false`.
    Manual,
    /// Bring the interface up when connected to specified SSID.
    /// Bring the interface down when disconnected from specified SSID.
    /// String `*` means any SSID.
    Wifi(Box<String>),
    /// Bring the interface up when specified SSID disconnected.
    /// Bring the interface down when specified SSID connected.
    /// String `*` is not valid, should use `InterfaceAutoConnect::Manual`
    /// with `state: down`.
    WifiNot(Box<String>),
}

impl Interface {
    /// Return `Some(true)` for interface should be up
    /// Return `Some(false)` for interface should be down
    /// Return None for interface not impacted or interface has no
    /// `auto-connect` defined.
    pub fn process_auto_connect(
        &self,
        event: &InterfaceLinkEvent,
        cur_ifaces: &Interfaces,
    ) -> Option<bool> {
        match self.base_iface().auto_connect.as_ref()? {
            InterfaceAutoConnect::Manual => None,
            InterfaceAutoConnect::AutoConnect => {
                let name_matched = self.kernel_iface_name() == event.iface_name;
                // When the kernel interface name does not match (e.g. the
                // device got a new name after unplug/replug), fall back to
                // matching the MAC address of the interface from the event.
                let mac_matched = !name_matched
                    && self.base_iface().identifier
                        == Some(InterfaceIdentifier::MacAddress)
                    && self.base_iface().mac_address.as_deref().is_some_and(
                        |saved_mac| {
                            let saved_mac = saved_mac.to_ascii_uppercase();
                            cur_ifaces
                                .kernel_ifaces
                                .get(&event.iface_name)
                                .is_some_and(|cur_iface| {
                                    let cur_base = cur_iface.base_iface();
                                    cur_base
                                        .permanent_mac_address
                                        .as_deref()
                                        .is_some_and(|m| {
                                            m.to_ascii_uppercase() == saved_mac
                                        })
                                        || cur_base
                                            .mac_address
                                            .as_deref()
                                            .is_some_and(|m| {
                                                m.to_ascii_uppercase()
                                                    == saved_mac
                                            })
                                })
                        },
                    );
                if !name_matched && !mac_matched {
                    None
                } else if event.is_delete {
                    // interface been removed, no action required.
                    None
                } else {
                    Some(event.is_up)
                }
            }
            InterfaceAutoConnect::Wifi(ssid) => {
                if cur_ifaces.iter().any(|cur_iface| {
                    if let Interface::WifiPhy(wifi_iface) = cur_iface {
                        wifi_iface.ssid() == Some(ssid.as_str())
                    } else {
                        false
                    }
                }) {
                    Some(true)
                } else {
                    Some(false)
                }
            }
            InterfaceAutoConnect::WifiNot(ssid) => {
                if cur_ifaces.iter().any(|cur_iface| {
                    if let Interface::WifiPhy(wifi_iface) = cur_iface {
                        wifi_iface.ssid() == Some(ssid.as_str())
                    } else {
                        false
                    }
                }) {
                    Some(false)
                } else {
                    Some(true)
                }
            }
        }
    }
}

impl InterfaceAutoConnect {
    pub fn is_wifi(&self) -> bool {
        matches!(self, Self::Wifi(_) | Self::WifiNot(_))
    }

    pub fn is_manual(&self) -> bool {
        matches!(self, Self::Manual)
    }
}

impl<'de> Deserialize<'de> for InterfaceAutoConnect {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let error_msg_prefix = "Expecting boolean or map with 'wifi' or \
                                'wifi-not' key for interface `auto-connect`";

        let v = serde_json::Value::deserialize(deserializer)?;
        if let Some(obj) = v.as_object() {
            if let Some(v) = obj.get("wifi") {
                Ok(Self::Wifi(Box::new(
                    <String>::deserialize(v)
                        .map_err(|e| serde::de::Error::custom(e.to_string()))?,
                )))
            } else if let Some(v) = obj.get("wifi-not") {
                Ok(Self::WifiNot(Box::new(
                    <String>::deserialize(v)
                        .map_err(|e| serde::de::Error::custom(e.to_string()))?,
                )))
            } else {
                Err(serde::de::Error::custom(format!(
                    "{error_msg_prefix}, but got {}",
                    obj.keys()
                        .map(|k| k.as_str())
                        .collect::<Vec<&str>>()
                        .join(" ")
                )))
            }
        } else if let Some(b) = v.as_bool() {
            if b {
                Ok(Self::AutoConnect)
            } else {
                Ok(Self::Manual)
            }
        } else if let Some(s) = v.as_str() {
            match s.to_lowercase().as_str() {
                "1" | "true" | "yes" | "on" | "y" => Ok(Self::AutoConnect),
                "0" | "false" | "no" | "off" | "n" => Ok(Self::Manual),
                _ => Err(serde::de::Error::custom(format!(
                    "{error_msg_prefix}, but got {s}"
                ))),
            }
        } else {
            Err(serde::de::Error::custom(format!(
                "{error_msg_prefix}, but got not boolean or map",
            )))
        }
    }
}

impl Serialize for InterfaceAutoConnect {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::AutoConnect => serializer.serialize_bool(true),
            Self::Manual => serializer.serialize_bool(false),
            Self::Wifi(v) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("wifi", v)?;
                map.end()
            }
            Self::WifiNot(v) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("wifi-not", v)?;
                map.end()
            }
        }
    }
}
