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
    /// Set by the monitor worker on the first event it sends for a
    /// wifi-phy. The event worker uses it to hand the saved WIFI profiles
    /// to the plugin, which then (re)creates its client on the new phy.
    #[serde(skip)]
    pub is_new_wifi_phy: bool,
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
            is_new_wifi_phy: false,
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
            is_new_wifi_phy: false,
        }
    }
}
