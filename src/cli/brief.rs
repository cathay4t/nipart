// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, fmt::Write as _FmtWrite};

use nipart::{
    BaseInterface, BondMode, Interface, InterfaceIdentifier,
    InterfaceLinkState, InterfaceState, InterfaceType, Interfaces,
    NetworkState, NipartClient, NipartInterface, NipartQueryOption, RouteEntry,
    WifiConfig,
};

use crate::CliError;

const INDENT: &str = "    ";
const LIST_SPLITER: &str = ",";
const RT_TABLE_MAIN: u32 = 254;

pub(crate) struct CommandBrief;

impl CommandBrief {
    pub(crate) const CMD: &str = "brief";

    pub(crate) fn new_cmd() -> clap::Command {
        clap::Command::new("brief")
            .alias("b")
            .about("Show brief info of configured or running interfaces")
            .arg(
                clap::Arg::new("IFNAME_OR_PROFILE")
                    .index(1)
                    .help("Show specific interface or profile only"),
            )
            .arg(
                clap::Arg::new("RUNNING")
                    .long("running")
                    .short('r')
                    .action(clap::ArgAction::SetTrue)
                    .help("Show all running interfaces instead"),
            )
    }

    pub(crate) async fn handle(
        matches: &clap::ArgMatches,
    ) -> Result<(), CliError> {
        handle(
            matches.get_flag("RUNNING"),
            matches
                .get_one::<String>("IFNAME_OR_PROFILE")
                .map(String::as_str),
        )
        .await
    }
}

#[derive(Default)]
struct CliIfaceBrief {
    index: u32,
    name: String,
    iface_type: String,
    driver: Option<String>,
    pci_address: Option<String>,
    controller: Option<String>,
    link_info: String,
    state: String,
    mac: String,
    permanent_mac: String,
    mtu: i64,
    ipv4: Vec<String>,
    ipv6: Vec<String>,
    ipv6_token: Option<String>,
    gw4: Vec<String>,
    gw6: Vec<String>,
    alt_names: Vec<String>,
}

impl CliIfaceBrief {
    fn list_show(briefs: &[CliIfaceBrief]) -> String {
        let mut ret = Vec::new();
        for brief in briefs {
            ret.push(format!(
                "{: >2}: {}: state {} mtu {}",
                brief.index, brief.name, brief.state, brief.mtu,
            ));
            if let Some(driver) = brief.driver.as_deref() {
                let mut drv_string = format!("{INDENT}driver {driver}");
                if let Some(pci_addr) = brief.pci_address.as_ref() {
                    write!(drv_string, " pci {pci_addr}").ok();
                }

                ret.push(drv_string);
            }

            let mut link_string =
                format!("{}link {}", INDENT, brief.iface_type);

            if !brief.link_info.is_empty() {
                write!(link_string, " {}", brief.link_info.as_str()).ok();
            }

            if let Some(ctrl) = brief.controller.as_ref() {
                write!(link_string, " controller {ctrl}").ok();
            }

            ret.push(link_string);

            if !brief.alt_names.is_empty() {
                ret.push(format!(
                    "{}altname {}",
                    INDENT,
                    brief.alt_names.join(",")
                ))
            }

            let mut mac_string = String::new();
            if !brief.mac.is_empty() {
                write!(mac_string, "{}mac {}", INDENT, brief.mac).ok();
                if !brief.permanent_mac.is_empty() {
                    write!(
                        mac_string,
                        " permanent_mac {}",
                        brief.permanent_mac
                    )
                    .ok();
                }
            }

            if !mac_string.is_empty() {
                ret.push(mac_string);
            }

            for ip in &brief.ipv4 {
                ret.push(format!("{INDENT}ipv4 {ip}"));
            }
            for gw in &brief.gw4 {
                ret.push(format!("{INDENT}gw4 {gw}"));
            }
            for ip in &brief.ipv6 {
                ret.push(format!("{INDENT}ipv6 {ip}"));
            }
            if let Some(token) = brief.ipv6_token.as_ref() {
                ret.push(format!("{INDENT}ipv6_token {token}"));
            }
            for gw in &brief.gw6 {
                ret.push(format!("{INDENT}gw6 {gw}"));
            }
        }
        ret.join("\n")
    }

    fn from_net_state(
        net_state: &NetworkState,
        saved_ifaces: Option<&[&Interface]>,
    ) -> Vec<Self> {
        let mut ret = Vec::new();
        let mut iface_to_gw4: HashMap<String, Vec<String>> = HashMap::new();
        let mut iface_to_gw6: HashMap<String, Vec<String>> = HashMap::new();

        for route in net_state.routes.running.iter().flatten() {
            if route.table_id != Some(RT_TABLE_MAIN) || !is_default_route(route)
            {
                continue;
            }
            let Some(gw) = route.next_hop_addr.as_deref() else {
                continue;
            };
            // Nipart maps a route without a gateway to "0.0.0.0" or "::",
            // which is not a real next hop.
            if gw == "0.0.0.0" || gw == "::" {
                continue;
            }
            let Some(iface_name) = route.next_hop_iface.as_deref() else {
                continue;
            };
            if gw.contains(':') {
                match iface_to_gw6.get_mut(iface_name) {
                    Some(gateways) => {
                        gateways.push(gw.to_string());
                    }
                    None => {
                        iface_to_gw6.insert(
                            iface_name.to_string(),
                            vec![gw.to_string()],
                        );
                    }
                }
            } else {
                match iface_to_gw4.get_mut(iface_name) {
                    Some(gateways) => {
                        gateways.push(gw.to_string());
                    }
                    None => {
                        iface_to_gw4.insert(
                            iface_name.to_string(),
                            vec![gw.to_string()],
                        );
                    }
                }
            }
        }

        for iface in net_state.ifaces.iter() {
            if iface.is_userspace() {
                continue;
            }
            if saved_ifaces
                .is_some_and(|saved| !is_configured_iface(saved, iface))
            {
                continue;
            }
            let base = iface.base_iface();
            ret.push(CliIfaceBrief {
                index: base.iface_index.unwrap_or_default(),
                driver: base.driver.clone(),
                pci_address: base.pci_address.clone(),
                iface_type: iface_type_str(iface.iface_type()),
                controller: base.controller.clone(),
                link_info: get_link_info(iface),
                name: iface.name().to_string(),
                state: iface_state_str(base),
                mac: base.mac_address.clone().unwrap_or_default(),
                permanent_mac: base
                    .permanent_mac_address
                    .clone()
                    .unwrap_or_default(),
                mtu: base.mtu.unwrap_or_default() as i64,
                ipv4: match &base.ipv4 {
                    Some(ip_info) => {
                        let mut addr_strs = Vec::new();
                        for addr in
                            ip_info.addresses.as_deref().unwrap_or_default()
                        {
                            addr_strs.push(format!(
                                "{}/{} valid_lft {} preferred_lft {}",
                                addr.ip,
                                addr.prefix_length,
                                addr.valid_life_time
                                    .as_deref()
                                    .unwrap_or("forever"),
                                addr.preferred_life_time
                                    .as_deref()
                                    .unwrap_or("forever"),
                            ));
                        }
                        addr_strs
                    }
                    None => Vec::new(),
                },
                ipv6: match &base.ipv6 {
                    Some(ip_info) => {
                        let mut addr_strs = Vec::new();
                        for addr in
                            ip_info.addresses.as_deref().unwrap_or_default()
                        {
                            addr_strs.push(format!(
                                "{}/{} valid_lft {} preferred_lft {}",
                                addr.ip,
                                addr.prefix_length,
                                addr.valid_life_time
                                    .as_deref()
                                    .unwrap_or("forever"),
                                addr.preferred_life_time
                                    .as_deref()
                                    .unwrap_or("forever"),
                            ));
                        }
                        addr_strs
                    }
                    None => Vec::new(),
                },
                ipv6_token: None,
                gw4: match &iface_to_gw4.get(iface.name()) {
                    Some(gws) => gws.to_vec(),
                    None => Vec::new(),
                },
                gw6: match &iface_to_gw6.get(iface.name()) {
                    Some(gws) => gws.to_vec(),
                    None => Vec::new(),
                },
                alt_names: base
                    .alt_names
                    .as_ref()
                    .map(|entries| {
                        entries.iter().map(|entry| entry.name.clone()).collect()
                    })
                    .unwrap_or_default(),
            })
        }
        ret.sort_by_key(|a| a.index);
        ret
    }
}

pub(crate) async fn handle(
    running: bool,
    ifname_or_profile: Option<&str>,
) -> Result<(), CliError> {
    let mut cli = NipartClient::new().await?;

    if running {
        let mut net_state = cli
            .query_network_state(NipartQueryOption::running())
            .await?;
        if let Some(name) = ifname_or_profile {
            let filtered_ifaces: Vec<Interface> = net_state
                .ifaces
                .iter()
                .filter(|iface| iface_matches_name(iface, name))
                .cloned()
                .collect();
            if filtered_ifaces.is_empty() {
                return Err(
                    format!("Interface or profile '{name}' not found").into()
                );
            }
            net_state.ifaces = Interfaces::new(filtered_ifaces);
        }
        let briefs = CliIfaceBrief::from_net_state(&net_state, None);
        if !briefs.is_empty() {
            println!("{}", CliIfaceBrief::list_show(&briefs));
        }
        return Ok(());
    }

    let saved_state =
        cli.query_network_state(NipartQueryOption::saved()).await?;

    let mut configured_ifaces: Vec<&Interface> = saved_state
        .ifaces
        .iter()
        .filter(|iface| !iface.is_absent())
        .collect();
    if let Some(name) = ifname_or_profile {
        configured_ifaces.retain(|iface| iface_matches_name(iface, name));
        if configured_ifaces.is_empty() {
            return Err(
                format!("Interface or profile '{name}' not found").into()
            );
        }
    }
    if configured_ifaces.is_empty() {
        return Ok(());
    }

    let net_state = cli
        .query_network_state(NipartQueryOption::running())
        .await?;

    let briefs =
        CliIfaceBrief::from_net_state(&net_state, Some(&configured_ifaces));
    if !briefs.is_empty() {
        println!("{}", CliIfaceBrief::list_show(&briefs));
    }
    Ok(())
}

fn iface_matches_name(iface: &Interface, name: &str) -> bool {
    iface.name() == name
        || iface.kernel_iface_name() == name
        || iface.base_iface().profile_name.as_deref() == Some(name)
}

fn is_configured_iface(saved_ifaces: &[&Interface], iface: &Interface) -> bool {
    saved_ifaces.iter().any(|saved| match saved {
        Interface::WifiCfg(wifi_cfg) => {
            if let Some(base_iface) =
                wifi_cfg.wifi.as_ref().and_then(|w| w.base_iface.as_deref())
            {
                base_iface == iface.name()
            } else if let Some(ssid) = wifi_cfg.ssid() {
                matches!(
                    iface,
                    Interface::WifiPhy(phy)
                        if phy.wifi.as_ref().is_some_and(|w| w.ssid == ssid)
                )
            } else {
                false
            }
        }
        Interface::OvsBridge(_) => saved.name() == iface.name(),
        _ => match saved.base_iface().identifier {
            Some(InterfaceIdentifier::MacAddress) => {
                let Some(saved_mac) = saved.base_iface().mac_address.as_deref()
                else {
                    return false;
                };
                let saved_mac = saved_mac.to_ascii_uppercase();
                base_iface_matches_mac(iface.base_iface(), &saved_mac)
            }
            _ => {
                let saved_name = if saved.kernel_iface_name().is_empty() {
                    saved.name()
                } else {
                    saved.kernel_iface_name()
                };
                saved_name == iface.name()
                    || saved_name == iface.kernel_iface_name()
            }
        },
    })
}

fn base_iface_matches_mac(base: &BaseInterface, saved_mac: &str) -> bool {
    base.mac_address
        .as_deref()
        .is_some_and(|mac| mac.to_ascii_uppercase() == saved_mac)
        || base
            .permanent_mac_address
            .as_deref()
            .is_some_and(|mac| mac.to_ascii_uppercase() == saved_mac)
}

fn get_link_info(iface: &Interface) -> String {
    match iface {
        Interface::Bond(bond) => {
            let Some(bond_conf) = bond.bond.as_ref() else {
                return String::new();
            };
            let ports = bond_conf
                .ports
                .as_deref()
                .map(|ports| {
                    ports
                        .iter()
                        .map(|port| port.name.as_str())
                        .collect::<Vec<_>>()
                        .join(LIST_SPLITER)
                })
                .unwrap_or_default();
            let mut bond_line = format!(
                "mode {} ports {}",
                bond_mode_str(bond_conf.mode),
                ports
            );
            if let Some(primary) = bond_conf
                .options
                .as_ref()
                .and_then(|o| o.primary.as_deref())
            {
                write!(bond_line, " primary {primary}").ok();
            }
            bond_line
        }
        Interface::LinuxBridge(br) => {
            if let Some(conf) = br.bridge.as_ref()
                && let Some(ports) = conf.ports.as_deref()
            {
                format!(
                    "ports {}",
                    ports
                        .iter()
                        .map(|port| port.name.as_str())
                        .collect::<Vec<_>>()
                        .join(LIST_SPLITER)
                )
            } else {
                String::new()
            }
        }
        Interface::Vrf(vrf) => {
            if let Some(conf) = vrf.vrf.as_ref() {
                format!(
                    "table {} ports {}",
                    conf.table_id.unwrap_or_default(),
                    conf.ports
                        .as_deref()
                        .unwrap_or_default()
                        .join(LIST_SPLITER)
                )
            } else {
                String::new()
            }
        }
        Interface::Ethernet(eth) => {
            if let Some(veth) = eth.veth.as_ref() {
                format!("peer {}", veth.peer)
            } else {
                String::new()
            }
        }
        Interface::Vlan(vlan) => {
            if let Some(conf) = vlan.vlan.as_ref() {
                format!(
                    "parent {} id {}",
                    conf.base_iface.as_deref().unwrap_or_default(),
                    conf.id.unwrap_or_default()
                )
            } else {
                String::new()
            }
        }
        Interface::Vxlan(vxlan) => {
            if let Some(conf) = vxlan.vxlan.as_ref() {
                format!(
                    "parent {} id {} remote {} dst_port {} local {}",
                    conf.base_iface.as_deref().unwrap_or_default(),
                    conf.id.unwrap_or_default(),
                    conf.remote.as_deref().unwrap_or_default(),
                    conf.destination_port.unwrap_or_default(),
                    conf.local.as_deref().unwrap_or_default()
                )
            } else {
                String::new()
            }
        }
        Interface::WifiPhy(phy) => {
            let Some(wifi) = phy.wifi.as_ref() else {
                return String::new();
            };
            let mut ret = String::new();
            if !wifi.ssid.is_empty() {
                write!(ret, "ssid {}", wifi.ssid).ok();
            }
            if let Some(generation) = wifi.generation.as_ref() {
                write!(ret, " gen {generation}").ok();
            }
            if let Some(freq) = wifi.frequency_mhz.as_ref() {
                write!(ret, " freq {freq}").ok();
            }
            if let Some(s) = wifi.signal_dbm.as_ref() {
                let perc = dbm_to_percentage(*s);
                write!(ret, " signal {s}dBm({perc}%)").ok();
            }
            if let Some(r) = wifi.rx_bitrate_mb.as_ref() {
                write!(ret, " rx {r}Mb/s").ok();
            }
            ret
        }
        _ => String::new(),
    }
}

fn iface_type_str(iface_type: &InterfaceType) -> String {
    match iface_type {
        InterfaceType::Bond => "bond",
        InterfaceType::LinuxBridge => "linux-bridge",
        InterfaceType::Dummy => "dummy",
        InterfaceType::Ethernet => "ethernet",
        InterfaceType::Hsr => "hsr",
        InterfaceType::Loopback => "loopback",
        InterfaceType::MacVlan => "macvlan",
        InterfaceType::MacVtap => "macvtap",
        InterfaceType::OvsBridge | InterfaceType::OvsInterface => "openvswitch",
        InterfaceType::Veth => "veth",
        InterfaceType::Vlan => "vlan",
        InterfaceType::Vrf => "vrf",
        InterfaceType::Vxlan => "vxlan",
        InterfaceType::InfiniBand => "infiniband",
        InterfaceType::Tun => "tun",
        InterfaceType::MacSec => "macsec",
        InterfaceType::Ipsec => "ipsec",
        InterfaceType::Xfrm => "xfrm",
        InterfaceType::IpVlan => "ipvlan",
        InterfaceType::WifiPhy => "wifi",
        InterfaceType::WifiCfg => "wifi-cfg",
        InterfaceType::Wireguard => "wireguard",
        InterfaceType::Unknown(v) => v.as_str(),
        _ => "unknown",
    }
    .to_string()
}

fn iface_state_str(base: &BaseInterface) -> String {
    match base.link_state {
        Some(InterfaceLinkState::Up) => "up",
        Some(InterfaceLinkState::Dormant) => "dormant",
        Some(InterfaceLinkState::Down) => "down",
        Some(InterfaceLinkState::LowerLayerDown) => "lower-layer-down",
        Some(InterfaceLinkState::Testing) => "testing",
        _ => match base.state {
            InterfaceState::Up => "up",
            InterfaceState::Down => "down",
            InterfaceState::Absent => "absent",
            InterfaceState::Ignore => "ignore",
            InterfaceState::UpIgnore => "up-ignore",
            InterfaceState::DownIgnore => "down-ignore",
            InterfaceState::Unknown => "unknown",
            _ => "unknown",
        },
    }
    .to_string()
}

fn is_default_route(route: &RouteEntry) -> bool {
    matches!(
        route.destination.as_deref(),
        Some("0.0.0.0/0") | Some("::/0")
    )
}

fn bond_mode_str(mode: Option<BondMode>) -> String {
    match mode {
        Some(BondMode::RoundRobin) => "balance-rr",
        Some(BondMode::ActiveBackup) => "active-backup",
        Some(BondMode::XOR) => "balance-xor",
        Some(BondMode::Broadcast) => "broadcast",
        Some(BondMode::LACP) => "802.3ad",
        Some(BondMode::TLB) => "balance-tlb",
        Some(BondMode::ALB) => "balance-alb",
        Some(BondMode::Unknown) | None => "unknown",
        Some(_) => "unknown",
    }
    .to_string()
}

fn dbm_to_percentage(dbm: i16) -> u8 {
    WifiConfig::signal_dbm_to_percent(dbm)
}

#[cfg(test)]
mod tests {
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
            eth.base.permanent_mac_address =
                Some("00:11:22:33:44:55".to_string());
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
}
