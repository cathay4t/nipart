// SPDX-License-Identifier: Apache-2.0

use nipart::{
    BaseInterface, EthernetInterface, Interface, InterfaceIdentifier,
    InterfaceState, InterfaceType, NetworkState, OvsBridgeInterface,
    WifiCfgInterface, WifiConfig, WifiPhyInterface,
};

use super::*;

fn saved_ethernet(name: &str) -> Interface {
    Interface::Ethernet(Box::new(EthernetInterface::new(
        BaseInterface::new(name.to_string(), InterfaceType::Ethernet),
        None,
    )))
}

fn current_ethernet(name: &str, index: u32) -> Interface {
    let mut base =
        BaseInterface::new(name.to_string(), InterfaceType::Ethernet);
    base.iface_index = Some(index);
    base.state = InterfaceState::Up;
    base.mtu = Some(1500);
    Interface::Ethernet(Box::new(EthernetInterface::new(base, None)))
}

#[test]
fn test_iface_matches_name_and_profile() {
    let saved = saved_ethernet("eth0");
    assert!(iface_matches_name(&saved, "eth0"));
    assert!(!iface_matches_name(&saved, "eth1"));

    let mut base =
        BaseInterface::new("eth0".to_string(), InterfaceType::Ethernet);
    base.profile_name = Some("profile1".to_string());
    let saved =
        Interface::Ethernet(Box::new(EthernetInterface::new(base, None)));
    assert!(iface_matches_name(&saved, "profile1"));
}

#[test]
fn test_is_configured_iface_name_matching() {
    let saved = saved_ethernet("eth0");
    assert!(is_configured_iface(&[&saved], &current_ethernet("eth0", 2)));
    assert!(!is_configured_iface(
        &[&saved],
        &current_ethernet("eth1", 3)
    ));
}

#[test]
fn test_is_configured_iface_mac_matching() {
    let mut base =
        BaseInterface::new("profile1".to_string(), InterfaceType::Ethernet);
    base.identifier = Some(InterfaceIdentifier::MacAddress);
    base.mac_address = Some("00:11:22:33:44:55".to_string());
    let saved =
        Interface::Ethernet(Box::new(EthernetInterface::new(base, None)));
    let mut current = current_ethernet("eth0", 2);
    if let Interface::Ethernet(eth) = &mut current {
        eth.base.mac_address = Some("00:11:22:33:44:55".to_string());
    }
    assert!(is_configured_iface(&[&saved], &current));

    if let Interface::Ethernet(eth) = &mut current {
        eth.base.mac_address = Some("00:11:22:33:44:66".to_string());
        eth.base.permanent_mac_address = Some("00:11:22:33:44:55".to_string());
    }
    assert!(is_configured_iface(&[&saved], &current));
}

#[test]
fn test_is_configured_iface_wifi_cfg_ssid_matching() {
    let mut wifi_cfg = WifiCfgInterface::new(BaseInterface::new(
        "ssid1".to_string(),
        InterfaceType::WifiCfg,
    ));
    wifi_cfg.wifi = Some(WifiConfig {
        ssid: "ssid1".to_string(),
        ..Default::default()
    });
    let saved = Interface::WifiCfg(Box::new(wifi_cfg));

    let current = Interface::WifiPhy(Box::new(WifiPhyInterface::new(
        "wlan0".to_string(),
        WifiConfig {
            ssid: "ssid1".to_string(),
            ..Default::default()
        },
    )));
    assert!(is_configured_iface(&[&saved], &current));

    let current = Interface::WifiPhy(Box::new(WifiPhyInterface::new(
        "wlan0".to_string(),
        WifiConfig {
            ssid: "ssid2".to_string(),
            ..Default::default()
        },
    )));
    assert!(!is_configured_iface(&[&saved], &current));
}

#[test]
fn test_from_net_state_uses_running_state() {
    let saved = saved_ethernet("eth0");
    let mut net_state = NetworkState::new();
    net_state.ifaces.push(current_ethernet("eth0", 2));

    let briefs = CliIfaceBrief::from_net_state(&net_state, Some(&[&saved]));
    assert_eq!(briefs.len(), 1);
    assert_eq!(briefs[0].index, 2);
    assert_eq!(briefs[0].name, "eth0");
    assert_eq!(briefs[0].iface_type, "ethernet");
}

#[test]
fn test_from_net_state_without_saved_ifaces_shows_all() {
    let mut net_state = NetworkState::new();
    net_state.ifaces.push(current_ethernet("eth0", 2));
    net_state.ifaces.push(current_ethernet("eth1", 3));

    let briefs = CliIfaceBrief::from_net_state(&net_state, None);
    assert_eq!(briefs.len(), 2);
    assert_eq!(briefs[0].name, "eth0");
    assert_eq!(briefs[1].name, "eth1");
}

#[test]
fn test_from_net_state_strips_userspace_ifaces() {
    let mut net_state = NetworkState::new();
    net_state.ifaces.push(current_ethernet("eth0", 2));
    let ovs_base =
        BaseInterface::new("ovs0".to_string(), InterfaceType::OvsBridge);
    net_state.ifaces.push(Interface::OvsBridge(Box::new(
        OvsBridgeInterface::new(ovs_base, None),
    )));

    let briefs = CliIfaceBrief::from_net_state(&net_state, None);
    assert_eq!(briefs.len(), 1);
    assert_eq!(briefs[0].name, "eth0");
}

#[test]
fn test_list_show() {
    let brief = CliIfaceBrief {
        index: 2,
        name: "eth0".to_string(),
        iface_type: "ethernet".to_string(),
        state: "up".to_string(),
        mtu: 1500,
        mac: "00:11:22:33:44:55".to_string(),
        ..Default::default()
    };
    let output = CliIfaceBrief::list_show(&[brief]);
    assert!(
        output.contains("2: eth0: state up mtu 1500"),
        "Unexpected output: {output}"
    );
    assert!(output.contains("link ethernet"));
    assert!(output.contains("mac 00:11:22:33:44:55"));
}
