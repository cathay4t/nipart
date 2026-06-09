// SPDX-License-Identifier: Apache-2.0

use std::time::SystemTime;

use serde::Serialize;

use crate::{
    Interface, InterfaceLinkState, InterfaceType, JsonDisplay, NipartInterface,
};

#[derive(Debug, Clone, PartialEq, Serialize, JsonDisplay)]
pub struct InterfaceLinkEvent {
    pub iface_name: String,
    pub iface_index: u32,
    pub iface_type: InterfaceType,
    pub is_up: bool,
    pub is_delete: bool,
    pub time_stamp: SystemTime,
    /// For WIFI interface only, SSID connected
    pub ssid: Option<String>,
}

impl InterfaceLinkEvent {
    pub fn new(
        iface_name: String,
        iface_index: u32,
        iface_type: InterfaceType,
        is_up: bool,
        ssid: Option<String>,
    ) -> Self {
        Self {
            iface_name,
            iface_index,
            iface_type,
            is_up,
            is_delete: false,
            time_stamp: SystemTime::now(),
            ssid,
        }
    }
}

impl From<&Interface> for InterfaceLinkEvent {
    fn from(iface: &Interface) -> Self {
        Self {
            iface_name: iface.kernel_iface_name().to_string(),
            iface_index: iface.base_iface().iface_index.unwrap_or_default(),
            iface_type: iface.iface_type().clone(),
            is_up: iface.base_iface().link_state
                == Some(InterfaceLinkState::Up),
            is_delete: iface.base_iface().state.is_absent(),
            time_stamp: SystemTime::now(),
            ssid: if let Interface::WifiPhy(wifi_iface) = iface {
                wifi_iface.ssid().map(|s| s.to_string())
            } else {
                None
            },
        }
    }
}
