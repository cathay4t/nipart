// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use nipart::{
    BaseInterface, Interface, InterfaceState, InterfaceType, NetworkState,
    WifiConfig, WifiPhyInterface,
};

use crate::{NipartWpaConn, apply::WifiLiveState};

impl NipartWpaConn {
    /// Convert the plugin's live shuli connection states into a network
    /// state carrying only wifi-phys shuli reports as connected.
    pub(crate) fn network_state_from_live_ifaces(
        live_ifaces: &HashMap<String, WifiLiveState>,
    ) -> NetworkState {
        let mut net_state = NetworkState::default();
        for (iface_name, live) in live_ifaces {
            let mut iface = WifiPhyInterface::default();
            iface.base =
                BaseInterface::new(iface_name.clone(), InterfaceType::WifiPhy);
            iface.base.state = InterfaceState::Up;
            iface.wifi = Some(WifiConfig {
                ssid: live.ssid.clone(),
                bssid: live.bssid.clone(),
                ..Default::default()
            });
            net_state.ifaces.push(Interface::WifiPhy(Box::new(iface)));
        }
        net_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_state_from_live_ifaces() {
        let mut live_ifaces = HashMap::new();
        live_ifaces.insert(
            "wlan0".to_string(),
            WifiLiveState {
                ssid: "Home-SSID".to_string(),
                bssid: None,
            },
        );

        let net_state =
            NipartWpaConn::network_state_from_live_ifaces(&live_ifaces);
        let ifaces: Vec<_> = net_state.ifaces.iter().collect();
        assert_eq!(ifaces.len(), 1);
        let Interface::WifiPhy(phy) = ifaces[0] else {
            panic!("expected wifi-phy interface");
        };
        assert_eq!(phy.ssid(), Some("Home-SSID"));
    }
}
