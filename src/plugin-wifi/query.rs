// SPDX-License-Identifier: Apache-2.0

use nipart::{
    BaseInterface, Interface, InterfaceType, Interfaces, NetworkState,
    NipartError, WifiPhyInterface,
};

use crate::NipartWpaConn;

impl NipartWpaConn {
    pub(crate) async fn query_network_state()
    -> Result<NetworkState, NipartError> {
        let mut net_state = NetworkState::default();

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
        _ifaces: &mut Interfaces,
    ) -> Result<(), NipartError> {
        // Without wpa_supplicant, we no longer have a persistent
        // record of configured networks.  Connection state is
        // managed by shuli WifiClient instances spawned during
        // apply.  Full query support will be added with the
        // connection manager in a future release.
        Ok(())
    }
}
