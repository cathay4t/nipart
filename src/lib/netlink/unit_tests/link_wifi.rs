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

#[test]
fn test_apply_wifi_marks_interface_type() {
    let message = link_message(2, "wlan0", LinkLayerType::Ether, LinkFlags::Up);
    let mut state = LinkState::from_messages(&[message]);
    state.apply_wifi("wlan0", Some(wifi_config()));
    let mut interfaces = state.into_interfaces();
    let interface = interfaces.drain(..).next().expect("Interface missing");
    assert_eq!(interface.iface_type(), &InterfaceType::WifiPhy);
    assert!(matches!(interface, Interface::WifiPhy(_)));
    if let Interface::WifiPhy(wifi) = interface {
        let config = wifi.wifi.expect("WIFI config missing");
        assert_eq!(config.ssid, "test-wifi");
        assert_eq!(config.signal_dbm, Some(-55));
        assert_eq!(config.signal_percent, Some(90));
    }
}

