// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};

use nipart::{
    ErrorKind, Interface, InterfaceType, NipartError, NipartInterface,
    WifiConfig,
};
use shuli::{
    NetworkConfig as ShuliNetworkConfig, WifiClient,
    WifiConfig as ShuliWifiConfig, WifiState,
};

/// Per-interface metadata kept by the plugin while the single shuli
/// `WifiClient` owns the actual connection state machines.
#[derive(Debug, Default)]
pub(crate) struct WifiIfaceState {
    /// Kernel if_index of the phy the client was bound to at creation.
    /// When the phy is recreated (e.g. wifi driver module reload), the
    /// cached client targets a device that no longer exists, so this
    /// value is compared against the current kernel if_index on each
    /// apply and the single client is recreated on mismatch.
    if_index: u32,
    desired_networks: Vec<ShuliNetworkConfig>,
}

/// The plugin-side wifi state: one shuli `WifiClient` for all wifi-phy
/// interfaces plus the per-interface metadata the apply flow needs.
#[derive(Default)]
pub(crate) struct WifiClientState {
    client: Option<WifiClient>,
    ifaces: HashMap<String, WifiIfaceState>,
}

impl WifiClientState {
    pub(crate) fn has_client(&self) -> bool {
        self.client.is_some()
    }

    /// Drive the single client until one interface reports a state
    /// change. Returns immediately when no client exists yet.
    pub(crate) async fn run_once(&mut self) -> Result<(), NipartError> {
        let Some(client) = self.client.as_mut() else {
            return Ok(());
        };
        let result = client.run_multi().await.map_err(|e| {
            NipartError::new(
                ErrorKind::PluginFailure,
                format!("WIFI client run failed: {e}"),
            )
        })?;
        let iface_name = &result.iface_name;
        if let Some(e) = result.error.as_ref() {
            log::warn!("WIFI {iface_name} error: {e}");
        }
        match result.state {
            WifiState::ConnectedWithoutOffloadRekey
            | WifiState::ConnectedWithOffloadRekey => {
                let ssid =
                    client.current_ssid_of(iface_name).unwrap_or("unknown");
                let bssid = client
                    .current_bssid_of(iface_name)
                    .map(|mac| mac_to_string(&mac))
                    .unwrap_or_else(|| "00:00:00:00:00:00".to_string());
                log::info!(
                    "WIFI connected on {iface_name}: SSID {ssid}, BSSID \
                     {bssid}"
                );
            }
            WifiState::Failed => {
                log::warn!("WIFI {iface_name} connection failed, retrying");
            }
            WifiState::FailedAuthentication => {
                log::error!(
                    "WIFI {iface_name} authentication failed, retrying"
                );
            }
            state => {
                log::trace!("WIFI {iface_name} state: {state:?}");
            }
        }
        Ok(())
    }

    /// Cleanly disconnect every managed interface.
    pub(crate) async fn shutdown(&mut self) {
        if let Some(client) = self.client.as_mut() {
            client.shutdown().await;
        }
    }

    pub(crate) async fn apply(
        &mut self,
        ifaces: &[Interface],
        force: bool,
    ) -> Result<(), NipartError> {
        let mut available_wifi_phys: Vec<String> = Vec::new();
        // Map of wifi-phy name to its current kernel if_index, used to
        // detect a phy that was recreated (e.g. wifi driver module
        // reload): the cached shuli client is then bound to a dead
        // device and must be restarted.
        let mut wifi_phys_if_index: HashMap<String, u32> = HashMap::new();
        {
            let mut filter = nispor::NetStateFilter::minimum();
            filter.iface = Some(nispor::NetStateIfaceFilter::minimum());
            if let Ok(np_state) =
                nispor::NetState::retrieve_with_filter_async(&filter).await
            {
                for np_iface in np_state.ifaces.values() {
                    if np_iface.iface_type == nispor::IfaceType::Wifi {
                        available_wifi_phys.push(np_iface.name.to_string());
                        wifi_phys_if_index
                            .insert(np_iface.name.to_string(), np_iface.index);
                    }
                }
            }
        }

        // Group every desired wifi config by the phy it binds to. All
        // SSIDs of one phy go into a single shuli network list.
        let mut desired: HashMap<String, Vec<&WifiConfig>> = HashMap::new();
        let mut iface_names_to_delete: HashSet<&str> = HashSet::new();
        let mut ssids_to_delete: HashSet<&str> = HashSet::new();

        for iface in ifaces {
            let wifi_cfg = match iface {
                Interface::WifiCfg(iface) => iface.wifi.as_ref(),
                Interface::WifiPhy(iface) => iface.wifi.as_ref(),
                _ => continue,
            };
            if iface.is_absent() || iface.is_down() {
                if iface.iface_type() == &InterfaceType::WifiPhy {
                    iface_names_to_delete.insert(iface.kernel_iface_name());
                } else if let Some(wifi_cfg) = wifi_cfg {
                    // wifi-cfg removal: drop the SSID from every phy
                    // that currently has it configured.
                    ssids_to_delete.insert(wifi_cfg.ssid.as_str());
                } else {
                    // An absent wifi-cfg strips its wifi section; the
                    // profile name is the SSID it was created for.
                    ssids_to_delete.insert(iface.name());
                }
                continue;
            } else if !iface.is_up() {
                return Err(NipartError::new(
                    ErrorKind::Bug,
                    format!(
                        "NipartWpaConn::apply(): Got invalid interface state: \
                         {iface}"
                    ),
                ));
            }
            let Some(wifi_cfg) = wifi_cfg else {
                continue;
            };
            log::trace!("Applying {wifi_cfg}");
            let iface_name = if iface.iface_type() == &InterfaceType::WifiPhy {
                iface.kernel_iface_name().to_string()
            } else if let Some(iface_name) = wifi_cfg.base_iface.as_ref() {
                iface_name.clone()
            } else if let Some(iface_name) = available_wifi_phys.first() {
                iface_name.clone()
            } else {
                log::warn!(
                    "WifiCfg interface {} has no base_iface specified, no \
                     wifi-phy available to bind to",
                    iface.name()
                );
                continue;
            };
            desired.entry(iface_name).or_default().push(wifi_cfg);
        }

        let mut recreate = false;

        // Remove clients for phys explicitly absent/down in this apply.
        for iface_name in &iface_names_to_delete {
            if self.ifaces.remove(*iface_name).is_some() {
                log::info!("Disconnecting WIFI on {iface_name}");
                recreate = true;
            }
        }

        // Update the per-interface desired state. A phy that was
        // recreated (or is gone) forces the single client to be rebuilt.
        let iface_names: Vec<String> = self.ifaces.keys().cloned().collect();
        let mut pending_networks: HashMap<String, Vec<ShuliNetworkConfig>> =
            HashMap::new();
        for iface_name in iface_names {
            let Some(iface_state) = self.ifaces.get_mut(&iface_name) else {
                continue;
            };
            if wifi_phys_if_index
                .get(&iface_name)
                .is_none_or(|if_index| *if_index != iface_state.if_index)
            {
                log::warn!(
                    "WIFI {iface_name} device changed or gone, restarting \
                     client"
                );
                self.ifaces.remove(&iface_name);
                recreate = true;
                continue;
            }
            // An apply that does not mention this phy (e.g. the daemon
            // re-applying the IP config of a wifi-phy after link up)
            // must not tear the connection down: only explicit
            // absent/down wifi-cfg entries remove SSIDs.
            let networks = match desired.get(&iface_name) {
                Some(wifi_cfgs) => build_shuli_networks(wifi_cfgs)?,
                None => {
                    let mut networks = iface_state.desired_networks.clone();
                    networks.retain(|network| {
                        !ssids_to_delete.contains(network.ssid.as_str())
                    });
                    networks
                }
            };
            if iface_state.desired_networks != networks
                || (force && !networks.is_empty())
            {
                pending_networks.insert(iface_name.clone(), networks);
            }
        }

        // Start managing phys that appear in the desired state for the
        // first time.
        for (iface_name, wifi_cfgs) in &desired {
            if self.ifaces.contains_key(iface_name) {
                continue;
            }
            let networks = build_shuli_networks(wifi_cfgs)?;
            let if_index =
                wifi_phys_if_index.get(iface_name).copied().unwrap_or(0);
            self.ifaces.insert(
                iface_name.clone(),
                WifiIfaceState {
                    if_index,
                    desired_networks: networks,
                },
            );
            recreate = true;
        }

        if self.client.is_none() && !self.ifaces.is_empty() {
            recreate = true;
        }

        if recreate {
            for (iface_name, networks) in pending_networks.drain() {
                if let Some(iface_state) = self.ifaces.get_mut(&iface_name) {
                    iface_state.desired_networks = networks;
                }
            }
            if let Some(client) = self.client.as_mut() {
                client.shutdown().await;
            }
            let configs: Vec<ShuliWifiConfig> = self
                .ifaces
                .iter()
                .map(|(iface_name, iface_state)| {
                    let mut config = ShuliWifiConfig::new(iface_name);
                    config.networks = iface_state.desired_networks.clone();
                    config
                })
                .collect();
            if configs.is_empty() {
                self.client = None;
            } else {
                log::info!(
                    "Starting WIFI client on {}",
                    configs
                        .iter()
                        .map(|config| config.iface_name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                match WifiClient::init_multi(configs).await {
                    Ok(client) => self.client = Some(client),
                    Err(e) => {
                        log::error!("WIFI init failed: {e}");
                        self.client = None;
                    }
                }
            }
            return Ok(());
        }

        // Update the existing client in place when no interface was
        // added, removed, or recreated.
        if let Some(client) = self.client.as_mut() {
            for (iface_name, networks) in pending_networks {
                if force && !networks.is_empty() {
                    log::info!(
                        "Restarting WIFI on {iface_name} to force connection"
                    );
                    if let Err(e) =
                        client.update_networks_of(&iface_name, Vec::new()).await
                    {
                        log::warn!(
                            "WIFI {iface_name} failed to update networks: {e}"
                        );
                        continue;
                    }
                }
                if networks.is_empty() {
                    log::info!(
                        "No WIFI network desired on {iface_name}, \
                         disconnecting"
                    );
                }
                if let Err(e) = client
                    .update_networks_of(&iface_name, networks.clone())
                    .await
                {
                    log::warn!(
                        "WIFI {iface_name} failed to update networks: {e}"
                    );
                    continue;
                }
                if let Some(iface_state) = self.ifaces.get_mut(&iface_name) {
                    iface_state.desired_networks = networks;
                }
            }
        }

        Ok(())
    }
}

fn mac_to_string(mac: &[u8; 6]) -> String {
    mac.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Convert nipart wifi configs into one shuli network list, keeping the
/// order of the desired interfaces and de-duplicating SSIDs. Conflicting
/// credentials for the same SSID on the same phy are an error.
fn build_shuli_networks(
    wifi_cfgs: &[&WifiConfig],
) -> Result<Vec<ShuliNetworkConfig>, NipartError> {
    let mut ret: Vec<ShuliNetworkConfig> = Vec::new();
    for wifi_cfg in wifi_cfgs {
        if let Some(existing) = ret.iter().find(|n| n.ssid == wifi_cfg.ssid) {
            if existing.password.as_deref() != wifi_cfg.password.as_deref() {
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "Conflicting WIFI config for SSID {} on the same \
                         interface",
                        wifi_cfg.ssid
                    ),
                ));
            }
            log::debug!("Ignoring duplicate WIFI SSID {}", wifi_cfg.ssid);
            continue;
        }
        let mut network = ShuliNetworkConfig::new(&wifi_cfg.ssid);
        if let Some(password) = wifi_cfg.password.as_deref() {
            network.set_password(password);
        }
        ret.push(network);
    }
    Ok(ret)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wifi_cfg(ssid: &str, password: Option<&str>) -> WifiConfig {
        WifiConfig {
            ssid: ssid.to_string(),
            password: password.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn build_shuli_networks_keeps_order_and_dedupes() {
        let cfgs = [
            wifi_cfg("A", None),
            wifi_cfg("B", Some("secret")),
            wifi_cfg("A", None),
        ];
        let refs: Vec<&WifiConfig> = cfgs.iter().collect();
        let networks = build_shuli_networks(&refs).unwrap();
        assert_eq!(networks.len(), 2);
        assert_eq!(networks[0].ssid, "A");
        assert_eq!(networks[0].password, None);
        assert_eq!(networks[1].ssid, "B");
        assert_eq!(networks[1].password.as_deref(), Some("secret"));
    }

    #[test]
    fn build_shuli_networks_rejects_conflicting_password() {
        let cfgs = [wifi_cfg("A", Some("one")), wifi_cfg("A", Some("two"))];
        let refs: Vec<&WifiConfig> = cfgs.iter().collect();
        assert!(build_shuli_networks(&refs).is_err());
    }
}
