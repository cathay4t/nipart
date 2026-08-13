// SPDX-License-Identifier: Apache-2.0

use nipart::{
    ErrorKind, InterfaceType, NetworkState, NipartError, NipartInterface,
    NipartIpcConnection, NipartNoDaemon, NipartQueryOption, NipartStateKind,
};

use super::commander::NipartCommander;

impl NipartCommander {
    pub(crate) async fn query_network_state(
        &mut self,
        conn: Option<&mut NipartIpcConnection>,
        opt: NipartQueryOption,
    ) -> Result<NetworkState, NipartError> {
        if let Some(conn) = conn {
            conn.log_debug(format!("querying network state with option {opt}"))
                .await;
        } else {
            log::debug!("querying network state with option {opt}");
        }
        match opt.kind {
            NipartStateKind::RunningNetworkState => {
                let mut net_state =
                    NipartNoDaemon::query_network_state(opt.clone()).await?;

                let plugins_net_states = self
                    .plugin_manager
                    .query_network_state(opt.clone(), &net_state)
                    .await?;

                for plugins_net_state in plugins_net_states {
                    net_state.merge(&plugins_net_state)?;
                }

                // Load user space from conf_manager
                let mut saved_state = self.conf_manager.query_state().await?;
                for (_, iface) in saved_state.ifaces.user_ifaces.drain() {
                    if iface.iface_type() == &InterfaceType::WifiCfg {
                        net_state.ifaces.push(iface);
                    }
                }

                // Apply saved config properties which cannot be queried
                // from kernel state:
                //  * `auto-connect`: daemon-only config stored in conf_manager.
                //  * `profile-name`: the logical name of the saved config
                //    managing this kernel interface.
                for (_, mut saved_iface) in
                    saved_state.ifaces.kernel_ifaces.drain()
                {
                    // The saved config of `identifier: mac-address`
                    // interface holds no `kernel-iface-name`(keyed by
                    // profile name), hence search by MAC address match.
                    let cur_iface =
                        if saved_iface.is_name_matching() {
                            net_state
                                .ifaces
                                .kernel_ifaces
                                .get_mut(saved_iface.kernel_iface_name())
                        } else {
                            net_state.ifaces.kernel_ifaces.values_mut().find(
                                |cur_iface| saved_iface.is_match(cur_iface),
                            )
                        };
                    let Some(cur_iface) = cur_iface else {
                        continue;
                    };
                    // Multiple saved configs may resolve to the same
                    // running interface (e.g. a name-matched and a
                    // MAC-matched config for the same NIC), and the
                    // `drain()` order is nondeterministic. Only fill
                    // missing values so a `None` never clobbers a set
                    // one.
                    if let Some(auto_connect) =
                        saved_iface.base_iface_mut().auto_connect.take()
                        && cur_iface.base_iface().auto_connect.is_none()
                    {
                        cur_iface.base_iface_mut().auto_connect =
                            Some(auto_connect);
                    }
                    if let Some(profile_name) =
                        saved_iface.base_iface().profile_name.as_ref()
                        && cur_iface.base_iface().profile_name.is_none()
                    {
                        cur_iface.base_iface_mut().profile_name =
                            Some(profile_name.clone());
                    }
                    if let Some(description) =
                        saved_iface.base_iface().description.as_ref()
                        && cur_iface.base_iface().description.is_none()
                    {
                        cur_iface.base_iface_mut().description =
                            Some(description.clone());
                    }
                }

                self.dhcpv4_manager.fill_dhcp_states(&mut net_state).await?;
                self.dhcpv6_manager.fill_dhcp_states(&mut net_state).await?;

                if !opt.include_secrets {
                    net_state.hide_secrets();
                }

                // TODO: Mark interface/routes not int saved state as ignored.
                Ok(net_state)
            }
            NipartStateKind::SavedNetworkState => {
                let mut state = self.conf_manager.query_state().await?;
                if !opt.include_secrets {
                    state.hide_secrets();
                }
                Ok(state)
            }
            _ => Err(NipartError::new(
                ErrorKind::NoSupport,
                format!("Unsupported query option: {}", opt.kind),
            )),
        }
    }
}
