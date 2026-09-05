// SPDX-License-Identifier: Apache-2.0

use nipart::{
    BaseInterface, EthernetInterface, InterfaceType, Interfaces,
    WifiCfgInterface, WifiConfig, WifiPhyInterface,
};

use super::*;

fn new_iface(name: &str) -> Interface {
    Interface::Ethernet(Box::new(EthernetInterface::new(
        BaseInterface::new(name.to_string(), InterfaceType::Ethernet),
        None,
    )))
}

fn new_iface_with_profile(name: &str, profile_name: &str) -> Interface {
    let mut iface = new_iface(name);
    iface.base_iface_mut().profile_name = Some(profile_name.to_string());
    iface
}

fn new_wifi_phy(name: &str) -> Interface {
    Interface::WifiPhy(Box::new(WifiPhyInterface::new(
        name.to_string(),
        WifiConfig::default(),
    )))
}

fn new_wifi_cfg(name: &str, base_iface: Option<&str>) -> Interface {
    let mut wifi_cfg = WifiCfgInterface::new(BaseInterface::new(
        name.to_string(),
        InterfaceType::WifiCfg,
    ));
    wifi_cfg.wifi = Some(WifiConfig {
        ssid: name.to_string(),
        base_iface: base_iface.map(|s| s.to_string()),
        ..Default::default()
    });
    Interface::WifiCfg(Box::new(wifi_cfg))
}

fn new_route(destination: &str, next_hop_iface: &str) -> RouteEntry {
    let mut rt = RouteEntry::default();
    rt.destination = Some(destination.to_string());
    rt.next_hop_iface = Some(next_hop_iface.to_string());
    rt
}

fn net_state_with_two_ifaces_and_routes() -> NetworkState {
    let mut net_state = NetworkState::new();
    net_state.ifaces = Interfaces::new(vec![
        new_iface("eth0"),
        new_iface_with_profile("wlan0", "HomeWiFi"),
    ]);
    net_state.routes.running = Some(vec![
        new_route("0.0.0.0/0", "eth0"),
        new_route("192.0.2.0/24", "wlan0"),
        new_route("198.51.100.0/24", "eth1"),
    ]);
    net_state.routes.config = Some(vec![
        new_route("10.0.0.0/8", "HomeWiFi"),
        new_route("172.16.0.0/12", "eth1"),
    ]);
    net_state
}

#[test]
fn test_filter_edit_state_by_kernel_name_keeps_matching_routes() {
    let net_state = net_state_with_two_ifaces_and_routes();
    let filtered =
        filter_edit_state(&net_state, Some("eth0"), "saved").unwrap();

    assert_eq!(filtered.ifaces.iter().count(), 1);
    assert_eq!(filtered.ifaces.iter().next().unwrap().name(), "eth0");

    let running = filtered.routes.running.unwrap();
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].destination.as_deref(), Some("0.0.0.0/0"));
    assert!(filtered.routes.config.is_none());
}

#[test]
fn test_filter_edit_state_by_profile_keeps_matching_routes() {
    let net_state = net_state_with_two_ifaces_and_routes();
    let filtered =
        filter_edit_state(&net_state, Some("HomeWiFi"), "saved").unwrap();

    assert_eq!(filtered.ifaces.iter().count(), 1);
    assert_eq!(
        filtered.ifaces.iter().next().unwrap().kernel_iface_name(),
        "wlan0"
    );

    let running = filtered.routes.running.unwrap();
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].destination.as_deref(), Some("192.0.2.0/24"));

    let config = filtered.routes.config.unwrap();
    assert_eq!(config.len(), 1);
    assert_eq!(config[0].destination.as_deref(), Some("10.0.0.0/8"));
}

#[test]
fn test_filter_edit_state_missing_name_fails() {
    let net_state = net_state_with_two_ifaces_and_routes();
    let err =
        filter_edit_state(&net_state, Some("missing"), "saved").unwrap_err();
    assert!(
        err.to_string()
            .contains("No interface or profile 'missing'")
    );
    assert!(err.to_string().contains("saved state"));
}

#[test]
fn test_filter_edit_state_without_name_keeps_full_state() {
    let net_state = net_state_with_two_ifaces_and_routes();
    let filtered = filter_edit_state(&net_state, None, "saved").unwrap();
    assert_eq!(filtered, net_state);
}

#[test]
fn test_filter_edit_state_wifi_phy_includes_matching_wifi_cfgs() {
    let mut net_state = NetworkState::new();
    net_state.ifaces = Interfaces::new(vec![
        new_wifi_phy("wlan0"),
        new_wifi_cfg("HomeWiFi", Some("wlan0")),
        new_wifi_cfg("GuestWiFi", Some("wlan1")),
        new_wifi_cfg("AnyWiFi", None),
    ]);
    net_state.routes.config = Some(vec![
        new_route("192.0.2.0/24", "wlan0"),
        new_route("198.51.100.0/24", "wlan1"),
    ]);

    let filtered =
        filter_edit_state(&net_state, Some("wlan0"), "current").unwrap();

    let names: Vec<&str> =
        filtered.ifaces.iter().map(|iface| iface.name()).collect();
    assert_eq!(names, vec!["wlan0", "HomeWiFi", "AnyWiFi"]);
    let routes = filtered.routes.config.unwrap();
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].destination.as_deref(), Some("192.0.2.0/24"));
}

#[test]
fn test_filter_edit_state_from_saved_wifi_cfgs() {
    let mut saved_state = NetworkState::new();
    saved_state.ifaces = Interfaces::new(vec![
        new_wifi_cfg("HomeWiFi", Some("wlan0")),
        new_wifi_cfg("GuestWiFi", Some("wlan1")),
    ]);
    saved_state.routes.config = Some(vec![
        new_route("192.0.2.0/24", "wlan0"),
        new_route("198.51.100.0/24", "wlan1"),
    ]);

    let filtered =
        filter_edit_state_from_saved_wifi_cfgs(&saved_state, "wlan0").unwrap();

    let names: Vec<&str> =
        filtered.ifaces.iter().map(|iface| iface.name()).collect();
    assert_eq!(names, vec!["HomeWiFi"]);
    let routes = filtered.routes.config.unwrap();
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].destination.as_deref(), Some("192.0.2.0/24"));
}

#[test]
fn test_filter_edit_state_from_saved_wifi_cfgs_missing_fails() {
    let mut saved_state = NetworkState::new();
    saved_state.ifaces = Interfaces::new(vec![new_iface("eth0")]);

    let err = filter_edit_state_from_saved_wifi_cfgs(&saved_state, "wlan0")
        .unwrap_err();
    assert!(err.to_string().contains("No interface or profile 'wlan0'"));
    assert!(err.to_string().contains("saved state"));
}
