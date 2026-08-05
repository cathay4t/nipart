// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use futures_channel::mpsc::UnboundedSender;
use nipart::{
    InterfaceType, NetworkState, NipartApplyOption, NipartError,
    NipartInterface, NipartNoDaemon, NipartQueryOption, NipartWifiScanOption,
    WifiConfig,
};

use super::{
    conf::NipartConfManager,
    daemon::NipartManagerCmd,
    dhcp::{NipartDhcpV4Manager, NipartDhcpV6Manager},
    event::NipartEventManager,
    monitor::NipartMonitorManager,
    plugin::NipartPluginManager,
    udev::udev_net_device_is_initialized,
};

const BOOTUP_NIC_CHECK_MAX_COUNT: u64 = 30;
const BOOTUP_NIC_CHECK_MAX_QUICK: u64 = 10;
// During quick retry, we retry every 0.5 second.
const BOOTUP_NIC_CHECK_INTERVAL_MS_QUICK: u64 = 500;
// After quick retry, we only retry every 2 seconds: the 10 seconds
// granularity used before meant a NIC that became udev-initialized right
// after a poll would not be configured until up to 10 seconds later,
// delaying the whole boot apply (and thus wait-online) on wired NICs.
const BOOTUP_NIC_CHECK_INTERVAL_SEC_SLOW: u64 = 2;

/// Commander manages all the task managers.
/// This struct is safe to clone and move to threads
#[derive(Debug, Clone)]
pub(crate) struct NipartCommander {
    pub(crate) dhcpv4_manager: NipartDhcpV4Manager,
    pub(crate) dhcpv6_manager: NipartDhcpV6Manager,
    pub(crate) monitor_manager: NipartMonitorManager,
    pub(crate) conf_manager: NipartConfManager,
    pub(crate) plugin_manager: NipartPluginManager,
    pub(crate) event_manager: NipartEventManager,
}

impl NipartCommander {
    pub(crate) async fn new(
        sender: UnboundedSender<NipartManagerCmd>,
    ) -> Result<Self, NipartError> {
        let mut ret = Self {
            dhcpv4_manager: NipartDhcpV4Manager::new().await?,
            dhcpv6_manager: NipartDhcpV6Manager::new().await?,
            monitor_manager: NipartMonitorManager::new(sender.clone()).await?,
            conf_manager: NipartConfManager::new().await?,
            plugin_manager: NipartPluginManager::new().await?,
            event_manager: NipartEventManager::new().await?,
        };
        ret.event_manager.set_commander(ret.clone()).await?;

        Ok(ret)
    }

    /// Shut down all task workers, waiting for each to finish so that
    /// their Drop-based cleanup (e.g. killing plugin child processes)
    /// completes before the daemon exits.
    pub(crate) async fn shutdown(&self) {
        self.plugin_manager.shutdown().await;
        self.monitor_manager.shutdown().await;
        self.dhcpv4_manager.shutdown().await;
        self.dhcpv6_manager.shutdown().await;
        self.conf_manager.shutdown().await;
        self.event_manager.shutdown().await;
    }

    // Workflow:
    //  1. Query current network state.
    //  2. For each non-virtual interface mentioned in saved state, if udev has
    //     it initialized, apply its config.
    //  3. Keep retry with timeout and interval for missing interfaces.
    pub(crate) async fn load_saved_state(&mut self) -> Result<(), NipartError> {
        self.monitor_manager.pause().await?;
        let result = self.load_saved_state_inner().await;
        // Always resume the monitor even when loading failed, otherwise the
        // daemon would stop reacting to interface link events (e.g. wifi
        // reconnect) for the rest of its life.
        self.monitor_manager.resume().await?;
        result
    }

    async fn load_saved_state_inner(&mut self) -> Result<(), NipartError> {
        let mut saved_state = self.conf_manager.query_state().await?;
        // Interfaces with `auto-connect: false` are only activated upon
        // explicit apply action, not at boot.
        remove_manual_activation(&mut saved_state);
        if saved_state.is_empty() {
            log::info!("Saved state is empty");
        } else {
            log::trace!("Loading saved state: {saved_state}");
            for retry_count in 0..BOOTUP_NIC_CHECK_MAX_COUNT {
                let kernel_iface_names =
                    get_initialized_nics(&saved_state).await?;

                let nic_ready_state =
                    remove_ready_state(&mut saved_state, &kernel_iface_names);

                if !nic_ready_state.is_empty() {
                    for iface in nic_ready_state.ifaces.iter() {
                        log::debug!(
                            "Applying saved state for interface {}/{}",
                            iface.name(),
                            iface.iface_type()
                        );
                    }
                    log::debug!("Applying saved state: {nic_ready_state}");
                    if let Err(e) = self
                        .apply_network_state(
                            None,
                            nic_ready_state.clone(),
                            NipartApplyOption::new().no_verify().memory_only(),
                        )
                        .await
                    {
                        // Do not abort the whole boot apply on failure (e.g.
                        // wifi plugin not ready for wpa_supplicant yet).
                        // Put the state back so the retry loop can try again.
                        log::warn!(
                            "Failed to apply saved state, will retry: {e}"
                        );
                        if let Err(e) = saved_state.merge(&nic_ready_state) {
                            log::error!(
                                "BUG: Failed to merge back unapplied saved \
                                 state: {e}"
                            );
                        }
                    } else {
                        log::debug!("Remaining saved state: {saved_state}");
                    }
                }
                if saved_state.is_empty() {
                    log::info!("All saved state applied successfully");
                    break;
                }

                if retry_count < BOOTUP_NIC_CHECK_MAX_QUICK {
                    tokio::time::sleep(std::time::Duration::from_millis(
                        BOOTUP_NIC_CHECK_INTERVAL_MS_QUICK,
                    ))
                    .await;
                } else {
                    tokio::time::sleep(std::time::Duration::from_secs(
                        BOOTUP_NIC_CHECK_INTERVAL_SEC_SLOW,
                    ))
                    .await;
                }
            }
            if !saved_state.is_empty() {
                log::error!(
                    "Failed to apply all saved state within {} retries, \
                     remaining: {saved_state}",
                    BOOTUP_NIC_CHECK_MAX_COUNT
                );
            }
        }
        Ok(())
    }

    pub(crate) async fn wifi_scan(
        &mut self,
        opt: NipartWifiScanOption,
    ) -> Result<Vec<WifiConfig>, NipartError> {
        self.plugin_manager.wifi_scan(opt).await
    }
}

async fn get_initialized_nics(
    saved_state: &NetworkState,
) -> Result<Vec<String>, NipartError> {
    let cur_state =
        NipartNoDaemon::query_network_state(NipartQueryOption::running())
            .await?;

    let mut ret = Vec::new();

    // The `kernel_ifaces` HashMap is keyed by the interface profile name for
    // MAC-address-matching interfaces whose kernel name is not resolved yet.
    // Use this key so `remove_ready_state()` can locate the interface.
    for (iface_key, iface) in saved_state
        .ifaces
        .kernel_ifaces
        .iter()
        .filter(|(_, i)| !i.is_virtual())
    {
        let cur_iface = cur_state
            .ifaces
            .kernel_ifaces
            .values()
            .find(|cur_iface| iface.is_match(cur_iface));

        if let Some(cur_iface) = cur_iface
            && let Some(cur_iface_index) = cur_iface.base_iface().iface_index
            && udev_net_device_is_initialized(cur_iface_index)
        {
            log::debug!(
                "Got Initialized NIC: {}/{}",
                cur_iface.name(),
                cur_iface.iface_type()
            );
            ret.push(iface_key.to_string());
        }
    }
    Ok(ret)
}

/// Return state for ready interfaces, and remove them from the original state.
fn remove_ready_state(
    state: &mut NetworkState,
    ready_kernel_iface_names: &[String],
) -> NetworkState {
    let mut ret = NetworkState::default();
    // HashMap of `<kernel_iface_name, iface_type>` for interface move
    // from old state to new state.
    let mut pending_ifaces: HashMap<String, Option<InterfaceType>> =
        HashMap::new();
    for kernel_iface_name in ready_kernel_iface_names {
        if let Some(iface) =
            state.ifaces.kernel_ifaces.get(kernel_iface_name.as_str())
            && iface.base_iface().controller.is_none()
        {
            // Use the HashMap key instead of `iface.kernel_iface_name()`
            // which is empty for unresolved MAC-address-matching interfaces.
            pending_ifaces.insert(kernel_iface_name.to_string(), None);
        }
    }

    // Include all virtual interface if not controller or controller has all
    // ports ready
    for iface in state.ifaces.iter().filter(|i| i.is_virtual()) {
        if iface.is_controller() {
            if let Some(ports) = iface.ports()
                && is_all_virtual_or_ready(
                    &ports,
                    ready_kernel_iface_names,
                    state,
                )
            {
                pending_ifaces.insert(
                    iface.kernel_iface_name().to_string(),
                    Some(iface.iface_type().clone()),
                );
                for port in ports {
                    pending_ifaces.insert(port.to_string(), None);
                }
            }
        } else {
            pending_ifaces.insert(
                iface.kernel_iface_name().to_string(),
                Some(iface.iface_type().clone()),
            );
        }
    }

    // Include routes of pending up interfaces
    ret.routes = state.routes.clone();
    ret.routes.config.get_or_insert_default().retain(|r| {
        if let Some(kernel_iface_name) = r.next_hop_iface.as_ref() {
            pending_ifaces.contains_key(kernel_iface_name)
        } else {
            false
        }
    });
    // Remove the ready routes from the original state so the retry loop can
    // terminate once all saved state has been extracted for apply.
    if let Some(state_rts) = state.routes.config.as_mut() {
        state_rts.retain(|r| {
            r.next_hop_iface
                .as_ref()
                .map(|n| !pending_ifaces.contains_key(n))
                .unwrap_or(true)
        });
    }

    for (iface_name, iface_type) in pending_ifaces.drain() {
        if let Some(iface) =
            state.ifaces.kernel_ifaces.remove(iface_name.as_str())
        {
            ret.ifaces.push(iface);
        } else if let Some(iface_type) = iface_type
            && let Some(iface) = state
                .ifaces
                .user_ifaces
                .remove(&(iface_name.clone(), iface_type))
        {
            // Userspace interfaces (e.g. `wifi-cfg`, OVS bridge) are moved
            // here so the boot retry loop can terminate after applying them
            // instead of keeping them as "remaining saved state" forever.
            ret.ifaces.push(iface);
        }
    }
    ret
}

fn is_all_virtual_or_ready(
    ports: &[&str],
    ready_kernel_iface_names: &[String],
    saved_state: &NetworkState,
) -> bool {
    for port in ports {
        let port = port.to_string();
        if !ready_kernel_iface_names.contains(&port)
            && saved_state
                .ifaces
                .kernel_ifaces
                .get(&port)
                .map(|i| i.is_virtual())
                != Some(true)
        {
            return false;
        }
    }
    true
}

/// Remove interfaces with `auto-connect: false` from the state applied at
/// boot: those interfaces are only activated upon explicit apply action.
/// Interfaces depending on an excluded interface(ports of excluded
/// controller or children of excluded parent) and routes pointing to them
/// are also removed, otherwise the boot retry loop would never terminate.
fn remove_manual_activation(state: &mut NetworkState) {
    let mut excluded: Vec<String> = state
        .ifaces
        .iter()
        .filter(|i| {
            i.base_iface()
                .auto_connect
                .as_ref()
                .is_some_and(|a| a.is_manual())
        })
        .map(|i| i.name().to_string())
        .collect();

    // Interfaces depending on an excluded interface cannot be activated at
    // boot either.
    let mut changed = true;
    while changed {
        changed = false;
        for iface in state.ifaces.iter() {
            if excluded.iter().any(|n| n == iface.name()) {
                continue;
            }
            if let Some(dependency) = iface
                .base_iface()
                .controller
                .as_deref()
                .or_else(|| iface.parent())
                && excluded.iter().any(|n| n == dependency)
            {
                excluded.push(iface.name().to_string());
                changed = true;
            }
        }
    }

    if excluded.is_empty() {
        return;
    }

    for iface_name in excluded.as_slice() {
        if state.ifaces.kernel_ifaces.remove(iface_name).is_some() {
            log::info!(
                "Skipping interface {iface_name} at boot due to \
                 `auto-connect: false`"
            );
        }
    }
    state
        .ifaces
        .user_ifaces
        .retain(|(iface_name, _), _| !excluded.iter().any(|n| n == iface_name));

    if let Some(rts) = state.routes.config.as_mut() {
        rts.retain(|rt| {
            rt.next_hop_iface
                .as_ref()
                .is_none_or(|n| !excluded.iter().any(|e| e == n))
        });
    }
}

#[cfg(test)]
mod tests {
    use nipart::{InterfaceType, NetworkState, NipartInterface};

    use super::{remove_manual_activation, remove_ready_state};

    #[test]
    fn test_remove_ready_state_moves_userspace_wifi_cfg() {
        // A `wifi-cfg` profile is a userspace interface: it must be moved
        // into the ready state so the boot retry loop can terminate, even
        // when no kernel NIC is ready yet.
        let mut state: NetworkState = serde_yaml::from_str(
            r#"---
            interfaces:
              - name: MyWiFi
                type: wifi-cfg
                state: up
                wifi:
                  ssid: MyWiFi
            "#,
        )
        .unwrap();

        let ready = remove_ready_state(&mut state, &[]);

        let wifi_cfgs: Vec<_> = ready
            .ifaces
            .iter()
            .filter(|i| i.iface_type() == &InterfaceType::WifiCfg)
            .collect();
        assert_eq!(wifi_cfgs.len(), 1);
        assert_eq!(wifi_cfgs[0].name(), "MyWiFi");
        assert!(state.ifaces.is_empty());
    }

    #[test]
    fn test_remove_ready_state_keeps_unready_kernel_iface() {
        // The non-virtual kernel interface without udev initialization must
        // stay in the saved state for later retry, while the userspace
        // `wifi-cfg` is moved out immediately.
        let mut state: NetworkState = serde_yaml::from_str(
            r#"---
            interfaces:
              - name: eth0
                type: ethernet
                state: up
              - name: MyWiFi
                type: wifi-cfg
                state: up
                wifi:
                  ssid: MyWiFi
            "#,
        )
        .unwrap();

        let ready = remove_ready_state(&mut state, &[]);

        assert_eq!(
            ready
                .ifaces
                .iter()
                .filter(|i| i.iface_type() == &InterfaceType::WifiCfg)
                .count(),
            1
        );
        // eth0 is not ready yet, it should still be pending in saved state.
        assert!(state.ifaces.kernel_ifaces.contains_key("eth0"));
        assert!(state.ifaces.user_ifaces.is_empty());
    }

    #[test]
    fn test_remove_manual_activation() {
        // Interfaces with `auto-connect: false`, their dependents, and
        // routes pointing to them are removed from the boot state.
        let mut state: NetworkState = serde_yaml::from_str(
            r#"---
            interfaces:
              - name: eth0
                type: ethernet
                state: up
                auto-connect: false
              - name: eth0.100
                type: vlan
                state: up
                vlan:
                  base-iface: eth0
                  id: 100
              - name: eth1
                type: ethernet
                state: up
                auto-connect: true
              - name: eth2
                type: ethernet
                state: up
              - name: bond0
                type: bond
                state: up
                auto-connect: false
                bond:
                  mode: balance-rr
              - name: eth3
                type: ethernet
                state: up
                controller: bond0
            routes:
              config:
                - destination: 192.0.2.0/24
                  next-hop-interface: eth0
                - destination: 198.51.100.0/24
                  next-hop-interface: eth1
            "#,
        )
        .unwrap();

        remove_manual_activation(&mut state);

        assert!(!state.ifaces.kernel_ifaces.contains_key("eth0"));
        // VLAN on top of an excluded interface is also excluded.
        assert!(!state.ifaces.kernel_ifaces.contains_key("eth0.100"));
        assert!(!state.ifaces.kernel_ifaces.contains_key("bond0"));
        // Port of an excluded controller is also excluded.
        assert!(!state.ifaces.kernel_ifaces.contains_key("eth3"));
        assert!(state.ifaces.kernel_ifaces.contains_key("eth1"));
        // Interface without `auto-connect` keeps the default auto behavior.
        assert!(state.ifaces.kernel_ifaces.contains_key("eth2"));

        let rts = state.routes.config.unwrap();
        assert_eq!(rts.len(), 1);
        assert_eq!(rts[0].next_hop_iface.as_deref(), Some("eth1"));
    }
}
