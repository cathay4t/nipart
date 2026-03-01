// SPDX-License-Identifier: Apache-2.0

use serde::{
    Deserialize, Deserializer, Serialize, Serializer, ser::SerializeMap,
};

use crate::JsonDisplay;

#[derive(Debug, Clone, PartialEq, Eq, Default, JsonDisplay)]
#[non_exhaustive]
pub enum InterfaceTrigger {
    /// Never bring interface up or down
    Never,
    /// Always bring interface up or down regardless its carrier state.
    #[default]
    Always,
    /// When carrier down, in order to monitor carrier, interface state will
    /// not changed to down state, only have IP stack disabled.
    /// When carrier up, Bring interface up and apply saved config.
    Carrier(Box<InterfaceTriggerCarrier>),
    /// Trigger the up/down action when specified SSID connected.
    /// String `*` means any SSID connected.
    WifiUp(Box<String>),
    /// Trigger the up/down action when specified SSID disconnected.
    /// String `*` means any SSID disconnected.
    WifiDown(Box<String>),
    /// Trigger the up/down action when SSID connected but not specified
    /// SSID. String `*` is not valid, should use `InterfaceTrigger::Never`
    /// instead.
    WifiUpNot(Box<String>),
}

impl InterfaceTrigger {
    pub fn is_wifi(&self) -> bool {
        matches!(
            self,
            Self::WifiUp(_) | Self::WifiDown(_) | Self::WifiUpNot(_)
        )
    }
}

impl<'de> Deserialize<'de> for InterfaceTrigger {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let error_msg_prefix = "Expecting 'never', 'always', 'carrier', \
                                'wifi-up', 'wifi-up-not', 'wifi-down' for \
                                interface trigger";

        let v = serde_json::Value::deserialize(deserializer)?;
        if let Some(obj) = v.as_object() {
            if let Some(v) = obj.get("wifi-up") {
                Ok(Self::WifiUp(Box::new(
                    <String>::deserialize(v)
                        .map_err(|e| serde::de::Error::custom(e.to_string()))?,
                )))
            } else if let Some(v) = obj.get("wifi-up-not") {
                Ok(Self::WifiUpNot(Box::new(
                    <String>::deserialize(v)
                        .map_err(|e| serde::de::Error::custom(e.to_string()))?,
                )))
            } else if let Some(v) = obj.get("wifi-down") {
                Ok(Self::WifiDown(Box::new(
                    <String>::deserialize(v)
                        .map_err(|e| serde::de::Error::custom(e.to_string()))?,
                )))
            } else if let Some(v) = obj.get("carrier") {
                Ok(Self::Carrier(Box::new(
                    <InterfaceTriggerCarrier>::deserialize(v)
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
        } else if let Some(obj_str) = v.as_str() {
            match obj_str {
                "never" => Ok(Self::Never),
                "always" => Ok(Self::Always),
                v => Err(serde::de::Error::custom(format!(
                    "{error_msg_prefix}, but got {v}",
                ))),
            }
        } else {
            Err(serde::de::Error::custom(format!(
                "{error_msg_prefix}, but got not string or map",
            )))
        }
    }
}

impl Serialize for InterfaceTrigger {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Never => serializer.serialize_str("never"),
            Self::Always => serializer.serialize_str("always"),
            Self::Carrier(v) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("carrier", v)?;
                map.end()
            }
            Self::WifiUp(v) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("wifi-up", v)?;
                map.end()
            }
            Self::WifiDown(v) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("wifi-down", v)?;
                map.end()
            }
            Self::WifiUpNot(v) => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("wifi-up-not", v)?;
                map.end()
            }
        }
    }
}

// TODO: Support delay for up/down action to prevent flipping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonDisplay)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[non_exhaustive]
pub struct InterfaceTriggerCarrier {
    // Seconds daemon should wait before take action after kernel announced
    // carrier down event. Default is 5 seconds if not defined to prevent
    // carrier flipping.
    // pub down_timeout_sec: Option<u32>,
    // Seconds daemon should wait before take action after kernel announced
    // carrier up event.
    // pub up_wait_sec: Option<u32>,
}
