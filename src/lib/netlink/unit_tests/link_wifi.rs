// SPDX-License-Identifier: Apache-2.0

use rtnetlink::packet_route::link::{LinkFlags, LinkLayerType};

use super::fixtures::link_message;
use crate::{
    Interface, InterfaceType, NipartInterface, WifiConfig,
    netlink::link::LinkState,
};

fn wifi_config() -> WifiConfig {
    let mut config = WifiConfig {
        ssid: "test-wifi".to_string(),
        bssid: Some("02:00:00:00:00:01".to_string()),
        frequency_mhz: Some(2412),
        generation: Some(6),
        rx_bitrate_mb: Some(144),
        tx_bitrate_mb: Some(130),
        signal_dbm: Some(-55),
        ..Default::default()
    };
    config.sanitize_signal();
    config
}

