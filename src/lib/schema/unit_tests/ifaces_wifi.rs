// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{Interface, NetworkState};

#[test]
fn test_hide_secrets_in_debug_wificfg() {
    let wifi_cfg = WifiConfig {
        ssid: "test-wifi".into(),
        password: Some("12345678".into()),
        ..Default::default()
    };
    let debug_str = format!("{:?}", wifi_cfg);
    println!("Debug string {:?}", debug_str);
    assert!(!debug_str.contains("12345678"));
}

#[test]
fn test_hide_secrets_in_display_wificfg() {
    let wifi_cfg = WifiConfig {
        ssid: "test-wifi".into(),
        password: Some("12345678".into()),
        ..Default::default()
    };
    let debug_str = format!("{}", wifi_cfg);
    println!("Display string {}", debug_str);
    assert!(!debug_str.contains("12345678"));
}

#[test]
fn test_hide_secrets_in_display_wifiiface() {
    let wifi_iface = WifiPhyInterface {
        base: Default::default(),
        wifi: Some(WifiConfig {
            ssid: "test-wifi".into(),
            password: Some("12345678".into()),
            ..Default::default()
        }),
    };
    let debug_str = format!("{}", wifi_iface);
    println!("Display string {}", debug_str);
    assert!(!debug_str.contains("12345678"));
}

#[test]
fn test_hide_secrets_in_display_iface() {
    let iface = Interface::WifiPhy(Box::new(WifiPhyInterface {
        base: Default::default(),
        wifi: Some(WifiConfig {
            ssid: "test-wifi".into(),
            password: Some("12345678".into()),
            ..Default::default()
        }),
    }));
    let debug_str = format!("{}", iface);
    println!("Display string {}", debug_str);
    assert!(!debug_str.contains("12345678"));
}

#[test]
fn test_hide_secrets_in_display_net_state() {
    let iface = Interface::WifiPhy(Box::new(WifiPhyInterface {
        base: Default::default(),
        wifi: Some(WifiConfig {
            ssid: "test-wifi".into(),
            password: Some("12345678".into()),
            ..Default::default()
        }),
    }));
    let mut net_state = NetworkState::new();
    net_state.ifaces.push(iface);
    let debug_str = format!("{}", net_state);
    println!("Display string {}", debug_str);
    assert!(!debug_str.contains("12345678"));
}
