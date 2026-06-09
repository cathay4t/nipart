// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use nipart::{
    Interface, InterfaceType, NetworkState, NipartApplyOption, NipartError,
    NipartIpcConnection, NipartPlugin, NipartPluginInfo, NipartQueryOption,
    NipartWifiScanOption, WifiConfig,
};

use crate::NipartWpaConn;

#[derive(Debug)]
pub(crate) struct NipartPluginWifi;

impl NipartPlugin for NipartPluginWifi {
    const PLUGIN_NAME: &'static str = "wifi";

    async fn init() -> Result<Self, NipartError> {
        Ok(Self {})
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
        NipartWpaConn::query_network_state().await
    }

    async fn apply_network_state(
        _plugin: &Arc<Self>,
        desired_state: NetworkState,
        _opt: NipartApplyOption,
        conn: &mut NipartIpcConnection,
    ) -> Result<(), NipartError> {
        conn.log_trace(format!(
            "WIFI plugin apply_network_state with state {desired_state}"
        ))
        .await;
        let ifaces: Vec<&Interface> = desired_state.ifaces.iter().collect();
        NipartWpaConn::apply(ifaces.as_slice()).await?;
        Ok(())
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
