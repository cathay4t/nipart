// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};

use nipart::{
    ErrorKind, Interface, InterfaceType, NipartError, NipartInterface,
    WifiConfig,
};

use crate::NipartWpaConn;

/// A live wifi connection managed by the plugin: the SSID it is
/// connected to, the keep-alive task owning the shuli `WifiClient`,
/// and a notification used to request a clean disconnect.
pub(crate) struct WifiConn {
    pub(crate) ssid: String,
    shutdown: std::sync::Arc<tokio::sync::Notify>,
    pub(crate) task: tokio::task::JoinHandle<()>,
}

impl WifiConn {
    /// Ask the keep-alive task to cleanly disconnect from the AP and
    /// wait until the DISCONNECT has been sent to the kernel.
    pub(crate) async fn disconnect(self) {
        self.shutdown.notify_one();
        let _ = self.task.await;
    }
}

/// Number of `WifiClient::run()` steps to attempt when connecting
/// before giving up.  Each step advances the shuli state machine
/// (scan → authenticate → connected) or waits out a retry backoff,
/// so the wall-clock time is roughly 3+ minutes per connect attempt.
const CONNECT_TIMEOUT_ITERS: u32 = 30;

impl NipartWpaConn {
    pub(crate) async fn apply(
        ifaces: &[Interface],
        wifi_conns: &mut HashMap<String, WifiConn>,
    ) -> Result<(), NipartError> {
        // Drop entries whose keep-alive task has already ended (e.g.
        // authentication failure): those connections are already gone.
        wifi_conns.retain(|_, conn| !conn.task.is_finished());

        let mut ssids_to_delete: HashSet<&str> = HashSet::new();
        let mut iface_names_to_delete: HashSet<&str> = HashSet::new();
        let mut wifi_cfg_to_add: Vec<(&str, &WifiConfig)> = Vec::new();

        let available_wifi_phys: Vec<String> = {
            let mut filter = nispor::NetStateFilter::minimum();
            filter.iface = Some(nispor::NetStateIfaceFilter::minimum());
            let mut ret = Vec::new();
            if let Ok(np_state) =
                nispor::NetState::retrieve_with_filter_async(&filter).await
            {
                for np_iface in np_state.ifaces.values() {
                    if np_iface.iface_type == nispor::IfaceType::Wifi {
                        ret.push(np_iface.name.to_string());
                    }
                }
            }
            ret
        };

        for iface in ifaces {
            let wifi_cfg = match iface {
                Interface::WifiCfg(iface) => iface.wifi.as_ref(),
                Interface::WifiPhy(iface) => iface.wifi.as_ref(),
                _ => continue,
            };
            if iface.is_absent() || iface.is_down() {
                if iface.iface_type() == &InterfaceType::WifiPhy {
                    iface_names_to_delete.insert(iface.kernel_iface_name());
                } else {
                    let ssid = if let Some(s) =
                        wifi_cfg.as_ref().map(|w| w.ssid.as_str())
                    {
                        s
                    } else {
                        iface.name()
                    };
                    ssids_to_delete.insert(ssid);
                }
            } else if iface.is_up() {
                let Some(wifi_cfg) = wifi_cfg else {
                    continue;
                };
                log::trace!("Applying {wifi_cfg}");
                if iface.iface_type() == &InterfaceType::WifiPhy {
                    wifi_cfg_to_add.push((iface.kernel_iface_name(), wifi_cfg));
                } else if let Some(iface_name) = wifi_cfg.base_iface.as_ref() {
                    wifi_cfg_to_add.push((iface_name, wifi_cfg));
                } else if let Some(iface_name) = available_wifi_phys.first() {
                    wifi_cfg_to_add.push((iface_name, wifi_cfg));
                } else {
                    log::warn!(
                        "WifiCfg interface {} has no base_iface specified, no \
                         wifi-phy available to bind to",
                        iface.name()
                    );
                }
            } else {
                return Err(NipartError::new(
                    ErrorKind::Bug,
                    format!(
                        "NipartWpaConn::apply(): Got invalid interface state: \
                         {iface}"
                    ),
                ));
            }
        }

        if !ssids_to_delete.is_empty() || !iface_names_to_delete.is_empty() {
            log::info!(
                "Disconnect requested for ssids={ssids_to_delete:?} \
                 ifaces={iface_names_to_delete:?}"
            );
        }
        for iface_name in &iface_names_to_delete {
            disconnect_iface(iface_name, wifi_conns).await;
        }
        for ssid in &ssids_to_delete {
            disconnect_ssid(ssid, wifi_conns).await;
        }

        for (iface_name, wifi_cfg) in &wifi_cfg_to_add {
            // Idempotency: re-applying the same SSID on a live
            // connection is a no-op.
            if let Some(conn) = wifi_conns.get(*iface_name)
                && conn.ssid == wifi_cfg.ssid
            {
                log::info!(
                    "Already connected to WIFI SSID {} on {}",
                    wifi_cfg.ssid,
                    iface_name
                );
                continue;
            }
            // A new config replaces any existing connection on the
            // interface: disconnect the old one (waiting for its
            // DISCONNECT to reach the kernel) before connecting.
            if let Some(conn) = wifi_conns.remove(*iface_name) {
                conn.disconnect().await;
            }
            let client = connect_wifi(iface_name, wifi_cfg).await?;
            wifi_conns.insert(
                (*iface_name).to_string(),
                spawn_keep_connected(iface_name, wifi_cfg.ssid.clone(), client),
            );
        }

        Ok(())
    }
}

async fn disconnect_iface(
    iface_name: &str,
    wifi_conns: &mut HashMap<String, WifiConn>,
) {
    if let Some(conn) = wifi_conns.remove(iface_name) {
        log::info!("Disconnecting WIFI on {iface_name}");
        conn.disconnect().await;
    }
}

async fn disconnect_ssid(
    ssid: &str,
    wifi_conns: &mut HashMap<String, WifiConn>,
) {
    let iface_names: Vec<String> = wifi_conns
        .iter()
        .filter(|(_, conn)| conn.ssid == ssid)
        .map(|(iface_name, _)| iface_name.clone())
        .collect();
    for iface_name in iface_names {
        if let Some(conn) = wifi_conns.remove(&iface_name) {
            log::info!("Disconnecting WIFI SSID {ssid} on {iface_name}");
            conn.disconnect().await;
        }
    }
}

/// Connect to the WIFI network and return the live `WifiClient` on
/// success.  The caller must keep the client alive (via
/// [`spawn_keep_connected`]) for the connection to persist: dropping
/// it would make shuli send `NL80211_CMD_DISCONNECT`.
async fn connect_wifi(
    iface_name: &str,
    wifi_cfg: &WifiConfig,
) -> Result<shuli::WifiClient, NipartError> {
    let ssid = wifi_cfg.ssid.clone();
    log::info!("Connecting to WIFI SSID {ssid} on {iface_name}");

    let mut config = shuli::WifiConfig::new(iface_name);
    config.add_network(&ssid, wifi_cfg.password.as_deref());

    let mut client = shuli::WifiClient::init(config).await.map_err(|e| {
        NipartError::new(ErrorKind::PluginFailure, format!("shuli init: {e}"))
    })?;

    for i in 0..CONNECT_TIMEOUT_ITERS {
        match client.run().await {
            Ok(shuli::WifiState::ConnectedWithoutOffloadRekey)
            | Ok(shuli::WifiState::ConnectedWithOffloadRekey) => {
                log::info!("Connected to WIFI SSID {ssid} on {iface_name}");
                return Ok(client);
            }
            Ok(shuli::WifiState::Failed) => {
                log::debug!(
                    "WIFI connect {ssid} failed, retry {}/{}",
                    i + 1,
                    CONNECT_TIMEOUT_ITERS
                );
            }
            Ok(shuli::WifiState::FailedAuthentication) => {
                return Err(NipartError::new(
                    ErrorKind::PluginFailure,
                    format!("WIFI authentication failed for SSID {ssid}"),
                ));
            }
            Ok(state) => {
                log::trace!("WIFI {ssid} state: {state:?}");
            }
            Err(e) => {
                log::warn!("WIFI {ssid} error: {e}");
            }
        }
    }

    Err(NipartError::new(
        ErrorKind::PluginFailure,
        format!("Timeout connecting to WIFI SSID {ssid} on {iface_name}"),
    ))
}

/// Spawn the keep-alive task for a freshly connected `WifiClient` and
/// return its [`WifiConn`] handle.  The task owns the client and keeps
/// calling `WifiClient::run()` so the connection persists: `run()`
/// drains nl80211 events (GTK rekeys, disconnects) while connected,
/// and reconnects automatically after a transient failure.  On
/// [`WifiConn::disconnect`] (or plugin shutdown) the task sends a
/// clean, awaited `NL80211_CMD_DISCONNECT` and exits promptly, waking
/// out of any `run()` backoff sleep.
fn spawn_keep_connected(
    iface_name: &str,
    ssid: String,
    client: shuli::WifiClient,
) -> WifiConn {
    let shutdown = std::sync::Arc::new(tokio::sync::Notify::new());
    let task_shutdown = shutdown.clone();
    let iface_name = iface_name.to_string();
    let task = tokio::spawn(async move {
        let mut client = client;
        loop {
            // `biased` is required: the shutdown branch must be polled
            // first so a notification is never lost to a concurrently
            // completing `run()` (which would leave no stored permit
            // and hang the caller's `WifiConn::disconnect()`).
            tokio::select! {
                biased;
                _ = task_shutdown.notified() => {
                    log::info!("Disconnecting WIFI on {iface_name}");
                    client.shutdown().await;
                    break;
                }
                result = client.run() => match result {
                    Ok(shuli::WifiState::ConnectedWithoutOffloadRekey)
                    | Ok(shuli::WifiState::ConnectedWithOffloadRekey) => {
                        // Connection is up; run() keeps draining events.
                    }
                    Ok(shuli::WifiState::Failed) => {
                        // run() already slept and reset to Init; the
                        // next iteration will try to reconnect.
                    }
                    Ok(shuli::WifiState::FailedAuthentication) => {
                        log::error!(
                            "WIFI authentication failed on {iface_name}, \
                             giving up"
                        );
                        // The client is dropped here, so shuli's `Drop`
                        // sends the disconnect on a detached thread
                        // (best-effort).  The connection is not up, so
                        // this is equivalent to the clean path above.
                        break;
                    }
                    Ok(state) => {
                        log::trace!(
                            "WIFI {iface_name} keep-alive state: {state:?}"
                        );
                    }
                    Err(e) => {
                        log::warn!("WIFI {iface_name} keep-alive error: {e}");
                    }
                },
            }
        }
    });
    WifiConn {
        ssid,
        shutdown,
        task,
    }
}
