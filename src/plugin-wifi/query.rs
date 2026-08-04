// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use nipart::{
    BaseInterface, Interface, InterfaceType, Interfaces, NetworkState,
    NipartError, WifiAuthType, WifiCfgInterface, WifiConfig, WifiPhyInterface,
};

use crate::{NipartWpaConn, dbus::NipartWpaSupDbus};

impl NipartWpaConn {
    pub(crate) async fn query_network_state()
    -> Result<NetworkState, NipartError> {
        let mut net_state = NetworkState::default();

        // Query nispor for WifiPhy interfaces so fill_wifi_cfg can
        // find and populate them with active wifi connection details.
        let mut filter = nispor::NetStateFilter::minimum();
        filter.iface = Some(nispor::NetStateIfaceFilter::minimum());
        if let Ok(np_state) =
            nispor::NetState::retrieve_with_filter_async(&filter).await
        {
            for np_iface in np_state.ifaces.values() {
                if np_iface.iface_type == nispor::IfaceType::Wifi {
                    let mut iface = WifiPhyInterface::default();
                    iface.base = BaseInterface::new(
                        np_iface.name.to_string(),
                        InterfaceType::WifiPhy,
                    );
                    net_state.ifaces.push(Interface::WifiPhy(Box::new(iface)));
                }
            }
        }

        Self::fill_wifi_cfg(&mut net_state.ifaces).await?;
        Ok(net_state)
    }

    pub(crate) async fn fill_wifi_cfg(
        ifaces: &mut Interfaces,
    ) -> Result<(), NipartError> {
        let dbus = match NipartWpaSupDbus::new().await {
            Ok(d) => d,
            Err(e) => {
                log::info!("{e}");
                return Ok(());
            }
        };
        let mut wifi_cfgs: HashMap<String, WifiConfig> = HashMap::new();

        let wpa_ifaces = dbus.get_ifaces().await?;
        for wpa_iface in &wpa_ifaces {
            for network in
                dbus.get_networks(wpa_iface.obj_path.as_str()).await?
            {
                let mut wifi_cfg = WifiConfig::from(network);
                wifi_cfg.base_iface = Some(wpa_iface.iface_name.to_string());
                wifi_cfg.auth_types =
                    wpa_iface.get_cur_auth_mode().map(|c| vec![c]);
                if let Some(auth_types) = wifi_cfg.auth_types.as_ref()
                    && auth_types.iter().any(|auth_type| {
                        matches!(
                            auth_type,
                            WifiAuthType::Wpa2Personal
                                | WifiAuthType::Wpa3Personal
                                | WifiAuthType::Wpa1
                                | WifiAuthType::Wep
                        )
                    })
                {
                    wifi_cfg.password = Some(
                        nipart::NetworkState::UNKNOWN_SECRET_STR.to_string(),
                    );
                }
                wifi_cfg.state = Some(wpa_iface.state.into());
                if let Some(exist_wifi_cfg) =
                    wifi_cfgs.get_mut(wifi_cfg.ssid.as_str())
                {
                    exist_wifi_cfg.base_iface = None;
                } else {
                    wifi_cfgs.insert(wifi_cfg.ssid.to_string(), wifi_cfg);
                }
            }
        }

        for wpa_iface in &wpa_ifaces {
            let Some(Interface::WifiPhy(iface)) =
                ifaces.kernel_ifaces.get_mut(wpa_iface.iface_name.as_str())
            else {
                continue;
            };
            if let Ok(bss) =
                dbus.get_current_bss(wpa_iface.obj_path.as_str()).await
                && let Some(ssid) = bss.ssid.as_ref()
                && let Some(wifi_cfg) = wifi_cfgs.get(ssid)
            {
                if let Some(kernel_wifi_cfg) = iface.wifi.as_mut() {
                    *kernel_wifi_cfg = wifi_cfg.merge(kernel_wifi_cfg)?;
                } else {
                    iface.wifi = Some(wifi_cfg.clone());
                }
            }
        }

        for wifi_cfg in wifi_cfgs.values() {
            ifaces.push(Interface::WifiCfg(Box::new(WifiCfgInterface::from(
                wifi_cfg.clone(),
            ))));
        }

        Ok(())
    }
}
