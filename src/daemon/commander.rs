// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use futures_channel::mpsc::UnboundedSender;
use nipart::{
    InterfaceType, NetworkState, NipartApplyOption, NipartError,
    NipartInterface, NipartNoDaemon, NipartQueryOption, NipartWifiScanOption,
    WifiConfig,
};

use super::{
    conf::NipartConfManager, daemon::NipartManagerCmd,
    dhcp::NipartDhcpV4Manager, event::NipartEventManager,
    monitor::NipartMonitorManager, plugin::NipartPluginManager,
    udev::udev_net_device_is_initialized,
};

const BOOTUP_NIC_CHECK_MAX_COUNT: u64 = 30;
const BOOTUP_NIC_CHECK_MAX_QUICK: u64 = 10;
// During quick retry, we retry every 0.5 second.
const BOOTUP_NIC_CHECK_INTERVAL_MS_QUICK: u64 = 500;
// After quick retry, we only retry every 10 seconds.
const BOOTUP_NIC_CHECK_INTERVAL_SEC_SLOW: u64 = 10;

/// Commander manages all the task managers.
/// This struct is safe to clone and move to threads
#[derive(Debug, Clone)]
pub(crate) struct NipartCommander {
    pub(crate) dhcpv4_manager: NipartDhcpV4Manager,
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
            monitor_manager: NipartMonitorManager::new(sender.clone()).await?,
            conf_manager: NipartConfManager::new().await?,
            plugin_manager: NipartPluginManager::new().await?,
            event_manager: NipartEventManager::new().await?,
        };
        ret.event_manager.set_commander(ret.clone()).await?;

        Ok(ret)
    }

    // Workflow:
    //  1. Query current network state.
    //  2. For each non-virtual interface mentioned in saved state, if udev has
    //     it initialized, apply its config.
    //  3. Keep retry with timeout and interval for missing interfaces.
    pub(crate) async fn load_saved_state(&mut self) -> Result<(), NipartError> {
        self.monitor_manager.pause().await?;
        let mut saved_state = self.conf_manager.query_state().await?;
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
                    self.apply_network_state(
                        None,
                        nic_ready_state,
                        NipartApplyOption::new().no_verify().memory_only(),
                    )
                    .await?;
                    log::debug!("Remaining saved state: {saved_state}");
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
        }
        self.monitor_manager.resume().await?;
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

    for iface in saved_state
        .ifaces
        .kernel_ifaces
        .values()
        .filter(|i| !i.is_virtual())
    {
        let kernel_iface_name = iface.kernel_iface_name();
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
            ret.push(kernel_iface_name.to_string());
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
            pending_ifaces.insert(iface.kernel_iface_name().to_string(), None);
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

    for (kernel_iface_name, _iface_type) in pending_ifaces.drain() {
        if let Some(iface) = state
            .ifaces
            .kernel_ifaces
            .remove(kernel_iface_name.as_str())
        {
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
