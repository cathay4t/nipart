// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use nipart::{
    ErrorKind, Interface, InterfaceType, NipartError, NipartInterface,
    WifiConfig,
};
use shuli::{
    NetworkConfig as ShuliNetworkConfig, WifiClient,
    WifiConfig as ShuliWifiConfig, WifiState,
};
use tokio::{
    sync::{
        Notify,
        mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    },
    task::JoinHandle,
};

use crate::NipartWpaConn;

/// A live wifi client managed by the plugin: one long-lived shuli
/// `WifiClient` per wifi-phy interface, reused across SSID changes.
/// The driver task owns the client; the apply worker only sends it the
/// new network list.
pub(crate) struct WifiConn {
    iface_name: String,
    /// Kernel if_index of the phy the client was bound to at creation.
    /// When the phy is recreated (e.g. wifi driver module reload), the
    /// cached client targets a device that no longer exists, so this
    /// value is compared against the current kernel if_index on each
    /// apply and the client is restarted on mismatch.
    if_index: u32,
    desired_networks: Vec<ShuliNetworkConfig>,
    cmd_tx: UnboundedSender<WifiConnCmd>,
    shutdown: Arc<Notify>,
    pub(crate) task: JoinHandle<()>,
}

enum WifiConnCmd {
    SetNetworks(Vec<ShuliNetworkConfig>),
}

impl WifiConn {
    fn new(
        iface_name: String,
        if_index: u32,
        desired_networks: Vec<ShuliNetworkConfig>,
        client: WifiClient,
    ) -> Self {
        let (cmd_tx, cmd_rx) = unbounded_channel();
        let shutdown = Arc::new(Notify::new());
        let task = tokio::spawn(run_driver(
            iface_name.clone(),
            client,
            cmd_rx,
            shutdown.clone(),
        ));
        Self {
            iface_name,
            if_index,
            desired_networks,
            cmd_tx,
            shutdown,
            task,
        }
    }

    fn has_same_networks(&self, networks: &[ShuliNetworkConfig]) -> bool {
        self.desired_networks == networks
    }

    /// Ask the driver to replace the network list. Returns `false` when
    /// the driver is already gone (e.g. it errored out).
    async fn set_networks(
        &mut self,
        networks: Vec<ShuliNetworkConfig>,
    ) -> bool {
        if self
            .cmd_tx
            .send(WifiConnCmd::SetNetworks(networks.clone()))
            .is_err()
        {
            log::warn!("WIFI driver for {} is gone", self.iface_name);
            return false;
        }
        self.desired_networks = networks;
        true
    }

    /// Ask the driver to cleanly disconnect from the AP and wait until
    /// the DISCONNECT has been sent to the kernel.
    pub(crate) async fn disconnect(self) {
        self.shutdown.notify_one();
        let _ = self.task.await;
    }

    /// Ask the driver to stop without waiting for it to finish: the phy
    /// was recreated (if_index changed) or is gone, so the cached client
    /// can only error out and may be stuck retrying on the dead device.
    /// Dropping the `JoinHandle` detaches the task; it exits once it
    /// observes the shutdown notification.
    fn shutdown_and_detach(self) {
        self.shutdown.notify_one();
    }
}

/// Drive one wifi-phy's `WifiClient` for the lifetime of the plugin:
/// keep calling `run()` so the connection persists, apply network-list
/// updates between runs, and cleanly disconnect on shutdown.
async fn run_driver(
    iface_name: String,
    mut client: WifiClient,
    mut cmd_rx: UnboundedReceiver<WifiConnCmd>,
    shutdown: Arc<Notify>,
) {
    // While no network is desired, `run()` is not called at all: the
    // client idles (and `update_networks` already disconnected).
    let mut has_networks = true;
    loop {
        tokio::select! {
            biased;
            _ = shutdown.notified() => {
                log::info!("Disconnecting WIFI on {iface_name}");
                client.shutdown().await;
                break;
            }
            cmd = cmd_rx.recv() => {
                let Some(cmd) = cmd else {
                    log::info!("WIFI driver channel closed on {iface_name}");
                    break;
                };
                match cmd {
                    WifiConnCmd::SetNetworks(networks) => {
                        has_networks = !networks.is_empty();
                        if let Err(e) =
                            client.update_networks(networks).await
                        {
                            log::warn!(
                                "WIFI {iface_name} failed to update \
                                 networks: {e}"
                            );
                        }
                    }
                }
            }
            result = client.run(), if has_networks => {
                match result {
                    Ok(
                        WifiState::ConnectedWithoutOffloadRekey
                        | WifiState::ConnectedWithOffloadRekey,
                    ) => {
                        let ssid = client.current_ssid();
                        let bssid = mac_to_string(&client.current_bssid());
                        log::info!(
                            "WIFI connected on {iface_name}: SSID {ssid}, \
                             BSSID {bssid}"
                        );
                    }
                    Ok(WifiState::Failed) => {
                        log::warn!(
                            "WIFI {iface_name} connection failed, retrying"
                        );
                    }
                    Ok(WifiState::FailedAuthentication) => {
                        log::error!(
                            "WIFI {iface_name} authentication failed, \
                             retrying"
                        );
                    }
                    Ok(state) => {
                        log::trace!("WIFI {iface_name} state: {state:?}");
                    }
                    Err(e) => {
                        log::warn!("WIFI {iface_name} error: {e}");
                    }
                }
            }
        }
    }
}

fn mac_to_string(mac: &[u8; 6]) -> String {
    mac.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

impl NipartWpaConn {
    pub(crate) async fn apply(
        ifaces: &[Interface],
        wifi_conns: &mut HashMap<String, WifiConn>,
        force: bool,
    ) -> Result<(), NipartError> {
        // Drop entries whose driver task has already ended.
        wifi_conns.retain(|_, conn| !conn.task.is_finished());

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

        for iface_name in &iface_names_to_delete {
            if let Some(conn) = wifi_conns.remove(*iface_name) {
                log::info!("Disconnecting WIFI on {iface_name}");
                conn.disconnect().await;
            }
        }

        // Update the clients that already exist: same list is a no-op,
        // a changed list is sent to the driver.
        let mut stale: Vec<String> = Vec::new();
        let iface_names: Vec<String> = wifi_conns.keys().cloned().collect();
        for iface_name in iface_names {
            let Some(conn) = wifi_conns.get_mut(&iface_name) else {
                continue;
            };
            // The phy may have been recreated (e.g. wifi driver module
            // reload): the cached WifiClient targets the old if_index and
            // would fail every nl80211 command with "No such device",
            // entering a long retry backoff. Drop it and let the client
            // be started again on the current device below.
            if wifi_phys_if_index
                .get(&iface_name)
                .is_none_or(|if_index| *if_index != conn.if_index)
            {
                log::warn!(
                    "WIFI {iface_name} device changed or gone, restarting \
                     client"
                );
                stale.push(iface_name);
                continue;
            }
            // An apply that does not mention this phy (e.g. the daemon
            // re-applying the IP config of a wifi-phy after link up)
            // must not tear the connection down: only explicit
            // absent/down wifi-cfg entries remove SSIDs.
            let networks = match desired.get(&iface_name) {
                Some(wifi_cfgs) => build_shuli_networks(wifi_cfgs)?,
                None => {
                    let mut networks = conn.desired_networks.clone();
                    networks.retain(|network| {
                        !ssids_to_delete.contains(network.ssid.as_str())
                    });
                    networks
                }
            };
            if conn.has_same_networks(&networks)
                && (!force || networks.is_empty())
            {
                log::info!("WIFI networks on {iface_name} unchanged");
                continue;
            }
            if force && !networks.is_empty() {
                log::info!(
                    "Restarting WIFI on {iface_name} to force connection"
                );
                if !conn.set_networks(Vec::new()).await {
                    stale.push(iface_name);
                    continue;
                }
            }
            if networks.is_empty() {
                log::info!(
                    "No WIFI network desired on {iface_name}, disconnecting"
                );
            }
            if !conn.set_networks(networks).await {
                stale.push(iface_name);
            }
        }
        for iface_name in stale {
            if let Some(conn) = wifi_conns.remove(&iface_name) {
                // Do not wait for the old driver: its device is gone or
                // was recreated, so it may be stuck retrying on the dead
                // device. The client on the new device is started below.
                conn.shutdown_and_detach();
            }
        }

        // Start a client for phys that appear in the desired state for
        // the first time. The client is kept alive across later SSID
        // changes: only its network list is updated.
        for (iface_name, wifi_cfgs) in &desired {
            if wifi_conns.contains_key(iface_name) {
                continue;
            }
            let networks = build_shuli_networks(wifi_cfgs)?;
            let mut shuli_cfg = ShuliWifiConfig::new(iface_name);
            shuli_cfg.networks = networks.clone();
            let client = match WifiClient::init(shuli_cfg).await {
                Ok(client) => client,
                Err(e) => {
                    log::error!("WIFI init failed on {iface_name}: {e}");
                    continue;
                }
            };
            let if_index =
                wifi_phys_if_index.get(iface_name).copied().unwrap_or(0);
            log::info!("Starting WIFI client on {iface_name}");
            wifi_conns.insert(
                iface_name.clone(),
                WifiConn::new(iface_name.clone(), if_index, networks, client),
            );
        }

        Ok(())
    }
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
