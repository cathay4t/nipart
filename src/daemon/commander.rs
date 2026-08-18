// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use futures_channel::mpsc::UnboundedSender;
use nipart::{
    BaseInterface, Interface, InterfaceIdentifier, InterfaceState,
    InterfaceType, NetworkState, NipartApplyOption, NipartError,
    NipartInterface, NipartNoDaemon, NipartQueryOption, NipartWifiScanOption,
    WifiScanResult,
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

// The boot apply retries for `BOOTUP_NIC_CHECK_MAX_QUICK` rounds of
// `BOOTUP_NIC_CHECK_INTERVAL_MS_QUICK` (5 seconds total) to give udev time
// to finish initializing NICs that exist but are still enumerating when the
// daemon first polls.  After that grace period the remaining saved configs
// (e.g. `identifier: mac-address` profiles whose NIC is not present) are
// left for the monitor worker: it emits a link event when the NIC appears
// and the event worker then applies the saved config.  We must not keep
// retrying indefinitely: a saved config whose NIC does not exist would
// otherwise delay the whole boot apply (and thus wait-online) for the full
// retry window.
const BOOTUP_NIC_CHECK_MAX_QUICK: u64 = 10;
const BOOTUP_NIC_CHECK_INTERVAL_MS_QUICK: u64 = 500;

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
    //  3. Retry for a short grace period so NICs that are still enumerating
    //     (udev not finished) get applied in the same boot pass.
    //  4. Leave the remaining saved configs (their NIC is not present) for the
    //     monitor worker: it emits a link event when the NIC appears and the
    //     event worker then applies the saved config.
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
            // Accumulate the saved interfaces successfully applied at
            // boot: after the loop their DHCP clients are restored (see
            // `restore_saved_dhcp_clients`).
            let mut boot_applied_ifaces: Vec<Interface> = Vec::new();
            for _ in 0..BOOTUP_NIC_CHECK_MAX_QUICK {
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
                        boot_applied_ifaces
                            .extend(nic_ready_state.ifaces.iter().cloned());
                    }
                }
                if saved_state.is_empty() {
                    log::info!("All saved state applied successfully");
                    break;
                }

                tokio::time::sleep(std::time::Duration::from_millis(
                    BOOTUP_NIC_CHECK_INTERVAL_MS_QUICK,
                ))
                .await;
            }
            // A DHCP-enabled interface whose lease survived the daemon
            // restart still carries its address in the kernel (reported
            // with `dhcp: true`), so the boot apply sees no diff and
            // never restarts the DHCP client - which is a userspace
            // process that died with the daemon.  The lease would then
            // expire without renewal.  Restore the DHCP clients now.
            self.restore_saved_dhcp_clients(&boot_applied_ifaces)
                .await?;
            if !saved_state.is_empty() {
                // The remaining saved configs target NICs that are not
                // present in the kernel (e.g. `identifier: mac-address`
                // profiles for NICs that are not installed on this host).
                // They are not applied at boot: register their monitor
                // watches so the monitor worker emits a link event when
                // such a NIC appears and the event worker then applies the
                // saved config.  Keep them in the saved state for that
                // path.
                self.monitor_manager
                    .setup_saved_state_monitors(&saved_state, true)
                    .await?;
                log::info!(
                    "Saved config for {} interface(s) without a present NIC \
                     is left for monitor worker to activate when the NIC \
                     appears: {saved_state}",
                    saved_state.ifaces.iter().count()
                );
            }
        }
        Ok(())
    }

    pub(crate) async fn wifi_scan(
        &mut self,
        mut opt: NipartWifiScanOption,
    ) -> Result<Vec<WifiScanResult>, NipartError> {
        if let Ok(saved_state) = self.conf_manager.query_state().await {
            for iface in saved_state.ifaces.iter() {
                let ssid = match iface {
                    Interface::WifiCfg(iface) => iface
                        .wifi
                        .as_ref()
                        .filter(|w| w.hidden)
                        .map(|w| w.ssid.clone()),
                    Interface::WifiPhy(iface) => iface
                        .wifi
                        .as_ref()
                        .filter(|w| w.hidden)
                        .map(|w| w.ssid.clone()),
                    _ => None,
                };
                if let Some(ssid) = ssid
                    && !opt.hidden_ssids.contains(&ssid)
                {
                    opt.hidden_ssids.push(ssid);
                }
            }
        }
        self.plugin_manager.wifi_scan(opt).await
    }

    /// Restore the DHCP clients for the saved interfaces applied at boot.
    ///
    /// [`Self::apply_network_state`] only (re)starts DHCP for interfaces
    /// whose kernel state changed.  A DHCP-enabled interface whose lease
    /// survived the daemon restart still carries its address in the
    /// kernel (reported with `dhcp: true`), so the merge sees no diff and
    /// the DHCP client - a userspace process that died with the daemon -
    /// is never restarted; the lease then expires without renewal.  This
    /// starts the DHCP client for every applied saved interface that has
    /// DHCP enabled and whose kernel interface already carries a DHCP
    /// address (i.e. the no-diff case; a cold boot has no address and is
    /// handled by the normal apply path).
    async fn restore_saved_dhcp_clients(
        &mut self,
        applied_ifaces: &[Interface],
    ) -> Result<(), NipartError> {
        let cur_state =
            NipartNoDaemon::query_network_state(NipartQueryOption::running())
                .await?;
        // Interfaces the boot apply already started a DHCP client for
        // (kernel state changed) must not be started again.
        let v4_running = self.dhcpv4_manager.running_ifaces().await?;
        let v6_running = self.dhcpv6_manager.running_ifaces().await?;
        for saved_iface in applied_ifaces {
            let base = saved_iface.base_iface();
            if base.state != InterfaceState::Up {
                continue;
            }
            // The DHCP client runs on the kernel interface the config
            // binds to: a wifi-cfg maps to the wifi-phy carrying its
            // SSID, all other configs to the interface matched by kernel
            // name or MAC address.
            let Some(cur_iface) =
                match_kernel_iface_for_saved_iface(saved_iface, &cur_state)
            else {
                continue;
            };
            let cur_base = cur_iface.base_iface();
            let iface_name = cur_iface.name().to_string();
            if base.ipv4.as_ref().is_some_and(|i| i.is_auto())
                && cur_base.ipv4.as_ref().is_some_and(|i| i.dhcp == Some(true))
                && !v4_running.contains(&iface_name)
            {
                log::info!(
                    "Restoring DHCPv4 client on interface {}({}) after daemon \
                     restart",
                    iface_name,
                    cur_iface.iface_type()
                );
                // The kernel state (`cur_base`) never carries the
                // config-only `auto_gateway` property, so inherit it from
                // the saved config, otherwise the restored client would
                // ignore `auto-gateway: false` and add the DHCP gateway
                // routes again.
                let dhcp_base_iface =
                    base_iface_for_dhcp_restore(cur_base, base);
                if let Err(e) =
                    self.dhcpv4_manager.start_iface_dhcp(&dhcp_base_iface).await
                {
                    // Do not abort the whole boot apply on a transient
                    // DHCP failure; the interface keeps its lease until
                    // it expires and a later apply can retry.
                    log::warn!(
                        "Failed to restore DHCPv4 client on interface \
                         {iface_name}: {e}"
                    );
                }
            }
            if base
                .ipv6
                .as_ref()
                .is_some_and(|i| i.is_enabled() && i.dhcp == Some(true))
                && cur_base.ipv6.as_ref().is_some_and(|i| i.dhcp == Some(true))
                && !v6_running.contains(&iface_name)
            {
                log::info!(
                    "Restoring DHCPv6 client on interface {}({}) after daemon \
                     restart",
                    iface_name,
                    cur_iface.iface_type()
                );
                if let Err(e) =
                    self.dhcpv6_manager.start_iface_dhcp(cur_base).await
                {
                    log::warn!(
                        "Failed to restore DHCPv6 client on interface \
                         {iface_name}: {e}"
                    );
                }
            }
        }
        Ok(())
    }
}

/// Build the base interface used to start the DHCPv4 client after a daemon
/// restart: the kernel state (which carries the MAC address and interface
/// index the client needs), but with the config-only `auto_gateway` property
/// inherited from the saved config — the kernel never reports it, so without
/// this the restored client would ignore `auto-gateway: false` and re-add
/// the DHCP gateway routes on the first renewal.
fn base_iface_for_dhcp_restore(
    kernel_base: &BaseInterface,
    saved_base: &BaseInterface,
) -> BaseInterface {
    let mut ret = kernel_base.clone();
    if let Some(ipv4) = ret.ipv4.as_mut() {
        ipv4.auto_gateway =
            saved_base.ipv4.as_ref().and_then(|i| i.auto_gateway);
    }
    ret
}

/// Find the kernel interface a saved config applies its DHCP to: a
/// wifi-cfg maps to the wifi-phy carrying its SSID, all other configs
/// match by kernel name or MAC address.
fn match_kernel_iface_for_saved_iface<'a>(
    saved_iface: &Interface,
    cur_state: &'a NetworkState,
) -> Option<&'a Interface> {
    if let Interface::WifiCfg(wifi_cfg) = saved_iface {
        let ssid = wifi_cfg.ssid()?;
        return cur_state.ifaces.kernel_ifaces.values().find(|cur_iface| {
            cur_iface.iface_type() == &InterfaceType::WifiPhy
                && matches!(
                    cur_iface,
                    Interface::WifiPhy(wifi_phy)
                        if wifi_phy.ssid() == Some(ssid)
                )
        });
    }
    let base = saved_iface.base_iface();
    let saved_mac = if base.identifier == Some(InterfaceIdentifier::MacAddress)
    {
        base.mac_address.as_deref().map(|m| m.to_ascii_uppercase())
    } else {
        None
    };
    cur_state.ifaces.kernel_ifaces.values().find(|cur_iface| {
        let cur_base = cur_iface.base_iface();
        saved_iface.kernel_iface_name() == cur_iface.kernel_iface_name()
            || saved_iface.name() == cur_iface.kernel_iface_name()
            || saved_mac.as_deref().is_some_and(|saved_mac| {
                cur_base
                    .mac_address
                    .as_deref()
                    .map(|m| m.to_ascii_uppercase() == saved_mac)
                    .unwrap_or(false)
            })
    })
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
    use nipart::{BaseInterface, InterfaceType, NetworkState, NipartInterface};

    use super::{
        base_iface_for_dhcp_restore, remove_manual_activation,
        remove_ready_state,
    };

    #[test]
    fn test_base_iface_for_dhcp_restore_inherits_auto_gateway() {
        let kernel_base: BaseInterface = rmsd_yaml::from_str(
            r#"---
            name: eth0
            type: ethernet
            state: up
            ipv4:
              enabled: true
              dhcp: true
            "#,
        )
        .unwrap();
        let saved_base: BaseInterface = rmsd_yaml::from_str(
            r#"---
            name: eth0
            type: ethernet
            state: up
            ipv4:
              enabled: true
              dhcp: true
              auto-gateway: false
            "#,
        )
        .unwrap();

        let ret = base_iface_for_dhcp_restore(&kernel_base, &saved_base);
        assert_eq!(ret.ipv4.as_ref().and_then(|i| i.auto_gateway), Some(false));
    }

    #[test]
    fn test_base_iface_for_dhcp_restore_defaults_to_none() {
        // Without `auto-gateway` in the saved config, the restored client
        // keeps the default behavior (gateway routes added).
        let kernel_base: BaseInterface = rmsd_yaml::from_str(
            r#"---
            name: eth0
            type: ethernet
            state: up
            ipv4:
              enabled: true
              dhcp: true
            "#,
        )
        .unwrap();
        // The saved config carries no IPv4 section at all.
        let saved_base: BaseInterface = rmsd_yaml::from_str(
            r#"---
            name: eth0
            type: ethernet
            state: up
            "#,
        )
        .unwrap();

        let ret = base_iface_for_dhcp_restore(&kernel_base, &saved_base);
        assert_eq!(ret.ipv4.as_ref().and_then(|i| i.auto_gateway), None);
    }

    #[test]
    fn test_remove_ready_state_moves_userspace_wifi_cfg() {
        // A `wifi-cfg` profile is a userspace interface: it must be moved
        // into the ready state so the boot retry loop can terminate, even
        // when no kernel NIC is ready yet.
        let mut state: NetworkState = rmsd_yaml::from_str(
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
        let mut state: NetworkState = rmsd_yaml::from_str(
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
        let mut state: NetworkState = rmsd_yaml::from_str(
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
