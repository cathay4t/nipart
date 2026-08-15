// SPDX-License-Identifier: Apache-2.0

use nipart::{
    Interface, InterfaceType, MergedNetworkState, NetworkState,
    NipartApplyOption, NipartError, NipartInterface, NipartIpcConnection,
    NipartNoDaemon,
};

use super::commander::NipartCommander;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

const RETRY_COUNT: usize = 10;
const RETRY_INTERVAL_MS: u64 = 500;

impl NipartCommander {
    pub(crate) async fn apply_network_state(
        &mut self,
        conn: Option<&mut NipartIpcConnection>,
        desired_state: NetworkState,
        opt: NipartApplyOption,
    ) -> Result<NetworkState, NipartError> {
        let saved_config = self.conf_manager.query_state().await?;
        self.apply_network_state_with_saved_config(
            conn,
            desired_state,
            opt,
            Some(saved_config),
        )
        .await
    }

    /// Apply desired state using an explicit saved state.
    ///
    /// `None` is used by explicit `npt up`/`npt down` actions: their desired
    /// state already contains the full saved profile, and passing the saved
    /// state here would re-inherit properties such as `auto-connect` and
    /// prevent the explicit action from overriding conditional activation.
    pub(crate) async fn apply_network_state_with_saved_config(
        &mut self,
        mut conn: Option<&mut NipartIpcConnection>,
        desired_state: NetworkState,
        opt: NipartApplyOption,
        saved_config: Option<NetworkState>,
    ) -> Result<NetworkState, NipartError> {
        if desired_state.is_empty() {
            log_info(
                conn.as_deref_mut(),
                "Desired state is empty, no action required".to_string(),
            )
            .await;
        }
        log_trace(
            conn.as_deref_mut(),
            format!("Apply {desired_state} with option {opt}"),
        )
        .await;

        let pre_apply_current_state = self
            .query_network_state(conn.as_deref_mut(), Default::default())
            .await?;

        let merged_state = MergedNetworkState::new(
            desired_state,
            pre_apply_current_state,
            saved_config,
            opt.clone(),
        )?;

        let state_to_save = merged_state.gen_state_for_save();
        log::debug!("State to save: {state_to_save}");

        let revert_state = merged_state.generate_revert()?;

        // TODO(Gris Ge): discard auto IPs

        // Suppress the monitor during applying
        self.monitor_manager.pause().await?;
        if let Err(e) = self
            .apply_merged_state(conn.as_deref_mut(), &merged_state)
            .await
        {
            log_warn(
                conn.as_deref_mut(),
                format!("Failed to apply desired state: {e}"),
            )
            .await;
            log_debug(
                conn.as_deref_mut(),
                format!("Failed to apply merged state: {merged_state}"),
            )
            .await;
            log_warn(
                conn.as_deref_mut(),
                "Rollback to state before apply".to_string(),
            )
            .await;
            log_trace(
                conn.as_deref_mut(),
                format!("Rollback to state before apply {revert_state}"),
            )
            .await;
            if let Err(e) =
                self.rollback(conn.as_deref_mut(), revert_state).await
            {
                log_error(
                    conn.as_deref_mut(),
                    format!("Failed to rollback: {e}"),
                )
                .await;
            }
            return Err(e);
        }

        if !merged_state.option.memory_only
            && let Err(e) = self.conf_manager.save_state(state_to_save).await
        {
            log_warn(
                conn.as_deref_mut(),
                format!("BUG: Failed to persistent desired state: {e}"),
            )
            .await;
        }

        let saved_state = self.conf_manager.query_state().await?;

        self.monitor_manager
            .setup_monitor(&merged_state, &saved_state)
            .await?;

        self.monitor_manager.resume().await?;

        let mut diff_state = match merged_state.gen_diff() {
            Ok(s) => s,
            Err(e) => {
                log_warn(
                    conn,
                    format!("Returning full state instead of diff state: {e}"),
                )
                .await;
                merged_state.gen_state_for_apply()
            }
        };
        diff_state.hide_secrets();

        self.try_set_daemon_online(Some(&saved_state), None).await?;

        Ok(diff_state)
    }

    async fn rollback(
        &mut self,
        mut conn: Option<&mut NipartIpcConnection>,
        revert_state: NetworkState,
    ) -> Result<(), NipartError> {
        let mut opt = NipartApplyOption::default();
        opt.no_verify = true;

        let current_state = self
            .query_network_state(conn.as_deref_mut(), Default::default())
            .await?;
        let mut merged_state = MergedNetworkState::new(
            revert_state,
            current_state,
            None,
            opt.clone(),
        )?;

        let apply_state = merged_state.gen_state_for_apply();

        NipartNoDaemon::apply_merged_state(&mut merged_state).await?;
        self.plugin_manager
            .apply_network_state(&apply_state, &opt)
            .await?;

        self.dhcpv4_manager
            .apply_dhcp_config(conn.as_deref_mut(), &merged_state)
            .await?;
        self.dhcpv6_manager
            .apply_dhcp_config(conn, &merged_state)
            .await?;

        Ok(())
    }

    async fn verify(
        &mut self,
        mut conn: Option<&mut NipartIpcConnection>,
        merged_state: &MergedNetworkState,
    ) -> Result<(), NipartError> {
        let mut post_apply_current_state = self
            .query_network_state(conn.as_deref_mut(), Default::default())
            .await?;
        // The wifi config is not stored into config manager yet. In order to
        // pass the verification, we need to pretend the wifi config is stored
        // in config manager.  An absent/down wifi-cfg must not be injected:
        // it is a virtual interface, so verification would reject it as
        // still present after the removal.
        for merged_iface in merged_state.ifaces.user_ifaces.values() {
            let Some(Interface::WifiCfg(iface)) = merged_iface.desired.as_ref()
            else {
                continue;
            };
            if iface.is_up() {
                post_apply_current_state
                    .ifaces
                    .push(Interface::WifiCfg(Box::new(*iface.clone())));
            } else {
                // The saved profile is only replaced after verification, so
                // drop it from the post-apply view when it is being removed.
                post_apply_current_state.ifaces.user_ifaces.remove(&(
                    iface.name().to_string(),
                    InterfaceType::WifiCfg,
                ));
            }
        }

        // The `auto-connect` is not stored into config manager yet. In order
        // to pass the verification, we need to pretend the `auto-connect` is
        // stored.
        for merged_iface in merged_state.ifaces.iter() {
            if let Some(post_apply_iface) = post_apply_current_state
                .ifaces
                .kernel_ifaces
                .get_mut(merged_iface.merged.kernel_iface_name())
            {
                post_apply_iface.base_iface_mut().auto_connect = merged_iface
                    .for_apply
                    .as_ref()
                    .and_then(|i| i.base_iface().auto_connect.clone());
            }
        }

        log_trace(
            conn,
            format!("Post apply network state: {post_apply_current_state}"),
        )
        .await;
        merged_state.verify(&post_apply_current_state)?;
        self.try_set_daemon_online(None, Some(&post_apply_current_state))
            .await?;
        Ok(())
    }

    // Apply state to plugin/dhcp/kernel and verify, but don't do these tasks:
    //  * Checkpoint rollback
    //  * Config save
    //  * Setup monitor session
    pub(crate) async fn apply_merged_state(
        &mut self,
        mut conn: Option<&mut NipartIpcConnection>,
        merged_state: &MergedNetworkState,
    ) -> Result<(), NipartError> {
        let apply_state = merged_state.gen_state_for_apply();

        log_trace(conn.as_deref_mut(), format!("apply_state {apply_state}"))
            .await;

        let mut merged_state_for_no_daemon = merged_state.clone();
        // Remove interfaces for conditional activating
        merged_state_for_no_daemon.remove_conditional_activation();

        NipartNoDaemon::apply_merged_state(&mut merged_state_for_no_daemon)
            .await?;
        self.plugin_manager
            .apply_network_state(&apply_state, &merged_state.option)
            .await?;

        self.dhcpv4_manager
            .apply_dhcp_config(conn.as_deref_mut(), merged_state)
            .await?;
        self.dhcpv6_manager
            .apply_dhcp_config(conn.as_deref_mut(), merged_state)
            .await?;

        let mut result: Result<(), NipartError> = Ok(());
        if !merged_state.option.no_verify {
            for cur_retry_count in 1..(RETRY_COUNT + 1) {
                result = self
                    .verify(conn.as_deref_mut(), &merged_state_for_no_daemon)
                    .await;
                if let Err(e) = &result {
                    log_info(
                        conn.as_deref_mut(),
                        format!(
                            "Retrying({cur_retry_count}/{RETRY_COUNT}) on \
                             verification error: {e}"
                        ),
                    )
                    .await;
                    tokio::time::sleep(std::time::Duration::from_millis(
                        RETRY_INTERVAL_MS,
                    ))
                    .await;
                } else {
                    break;
                }
            }
        }
        result
    }
}
