// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use nipart::{
    ErrorKind, Interface, InterfaceType, NetworkState, NipartApplyOption,
    NipartError, NipartIpcConnection, NipartPlugin, NipartPluginInfo,
    NipartQueryOption, NipartWifiScanOption, WifiConfig,
};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use crate::NipartWpaConn;

#[derive(Debug)]
pub(crate) struct NipartPluginWifi {
    apply_tx: tokio::sync::mpsc::UnboundedSender<Vec<Interface>>,
}

/// Dedicated worker processing wifi apply requests serially in arrival
/// order. Slow operations (wpa_supplicant config change, active scan) are
/// performed here so that `apply_network_state()` never blocks the daemon
/// connection handling, and `query_network_state()` is never stalled by an
/// on-going apply.
async fn apply_worker(mut rx: UnboundedReceiver<Vec<Interface>>) {
    while let Some(ifaces) = rx.recv().await {
        if let Err(e) = NipartWpaConn::apply(ifaces.as_slice()).await {
            log::error!("WIFI plugin failed to apply state: {e}");
        }
    }
}

impl NipartPlugin for NipartPluginWifi {
    const PLUGIN_NAME: &'static str = "wifi";

    async fn init() -> Result<Self, NipartError> {
        let (apply_tx, apply_rx) = unbounded_channel();
        tokio::spawn(apply_worker(apply_rx));
        Ok(Self { apply_tx })
    }

    async fn plugin_info(
        _plugin: &Arc<Self>,
    ) -> Result<NipartPluginInfo, NipartError> {
        Ok(NipartPluginInfo::new(
            "wifi".to_string(),
            "0.1.0".to_string(),
            vec![InterfaceType::WifiCfg, InterfaceType::WifiPhy],
        ))
    }

    async fn query_network_state(
        _plugin: &Arc<Self>,
        _opt: NipartQueryOption,
        _cur_net_state: &NetworkState,
        conn: &mut NipartIpcConnection,
    ) -> Result<NetworkState, NipartError> {
        conn.log_trace("WIFI plugin query_network_state".to_string())
            .await;
        // Query only reads the live wpa_supplicant configuration and its
        // saved scan results, it never triggers a scan.
        NipartWpaConn::query_network_state().await
    }

    async fn apply_network_state(
        plugin: &Arc<Self>,
        mut desired_state: NetworkState,
        _opt: NipartApplyOption,
        conn: &mut NipartIpcConnection,
    ) -> Result<(), NipartError> {
        conn.log_trace(format!(
            "WIFI plugin apply_network_state with state {desired_state}"
        ))
        .await;
        let ifaces: Vec<Interface> = desired_state.ifaces.drain().collect();
        // Never block: enqueue the request to the dedicated apply worker
        // and return immediately. The daemon verification stage waits and
        // retries until the applied state matches the desired state.
        plugin.apply_tx.send(ifaces).map_err(|e| {
            NipartError::new(
                ErrorKind::Bug,
                format!("Failed to enqueue wifi apply request: {e}"),
            )
        })
    }

    async fn wifi_scan(
        _plugin: &Arc<Self>,
        opt: NipartWifiScanOption,
        conn: &mut NipartIpcConnection,
    ) -> Result<Vec<WifiConfig>, NipartError> {
        conn.log_trace(format!("WIFI plugin wifi_scan with option {opt}"))
            .await;
        NipartWpaConn::wifi_scan(opt.iface_name.as_deref()).await
    }
}
