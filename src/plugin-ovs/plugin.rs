// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use nipart::{
    ErrorKind, InterfaceType, NetworkState, NipartApplyOption, NipartError,
    NipartInterface, NipartIpcConnection, NipartPlugin, NipartPluginInfo,
    NipartQueryOption,
};

use super::{db::OvsDbConnection, query::query_ovs_state};

#[derive(Debug)]
pub(crate) struct NipartPluginOvs;

impl NipartPlugin for NipartPluginOvs {
    const PLUGIN_NAME: &'static str = "ovs";

    async fn init() -> Result<Self, NipartError> {
        Ok(Self {})
    }

    async fn plugin_info(
        _plugin: &Arc<Self>,
    ) -> Result<NipartPluginInfo, NipartError> {
        Ok(NipartPluginInfo::new(
            "ovs".to_string(),
            "0.1.0".to_string(),
            vec![InterfaceType::OvsBridge, InterfaceType::OvsInterface],
        ))
    }

    async fn query_network_state(
        _plugin: &Arc<Self>,
        opt: NipartQueryOption,
        cur_net_state: &NetworkState,
        conn: &mut NipartIpcConnection,
    ) -> Result<NetworkState, NipartError> {
        conn.log_trace("OVS plugin query_network_state".to_string())
            .await;
        query_ovs_state(opt, cur_net_state).await
    }

    async fn apply_network_state(
        _plugin: &Arc<Self>,
        desired_state: NetworkState,
        _opt: NipartApplyOption,
        conn: &mut NipartIpcConnection,
    ) -> Result<(), NipartError> {
        conn.log_trace("OVS plugin apply_network_state".to_string())
            .await;
        if !desired_state.ifaces.iter().any(|i| {
            matches!(
                i.iface_type(),
                InterfaceType::OvsBridge | InterfaceType::OvsInterface
            )
        }) {
            return Ok(());
        }
        if OvsDbConnection::new().await.is_err() {
            return Err(NipartError::new(
                ErrorKind::DependencyError,
                "OVS daemon is not running".to_string(),
            ));
        }
        // TODO(Gris Ge): Implement OVS apply
        Ok(())
    }
}
