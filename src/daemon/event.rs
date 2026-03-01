// SPDX-License-Identifier: Apache-2.0

use std::time::SystemTime;

use nipart::{
    BaseInterface, Interface, InterfaceIpv4, InterfaceIpv6, InterfaceState,
    InterfaceTrigger, InterfaceType, Interfaces, JsonDisplay,
    MergedNetworkState, NetworkState, NipartError, NipartNoDaemon,
    NmstateApplyOption, NmstateInterface, NmstateQueryOption,
};
use serde::Serialize;

use super::commander::NipartCommander;

#[derive(Debug, Clone, PartialEq, Serialize, JsonDisplay)]
pub(crate) struct NipartLinkEvent {
    pub iface_name: String,
    pub iface_index: u32,
    pub iface_type: InterfaceType,
    pub is_up: bool,
    pub time_stamp: SystemTime,
    /// For WIFI interface only, SSID connected
    pub ssid: Option<String>,
}

impl NipartLinkEvent {
    pub(crate) fn new(
        iface_name: String,
        iface_index: u32,
        iface_type: InterfaceType,
        is_up: bool,
        ssid: Option<String>,
    ) -> Self {
        Self {
            iface_name,
            iface_index,
            iface_type,
            is_up,
            time_stamp: SystemTime::now(),
            ssid,
        }
    }
}

impl NipartCommander {
    pub(crate) async fn handle_link_event(
        &mut self,
        mut event: NipartLinkEvent,
    ) -> Result<(), NipartError> {
        log::trace!("Handle link event {event}");
        let saved_state = self.conf_manager.query_state().await?;
        let cur_state =
            NipartNoDaemon::query_network_state(NmstateQueryOption::running())
                .await?;

        let cur_iface = cur_state
            .ifaces
            .get(&event.iface_name, Some(&event.iface_type));
        if let Some(cur_iface) = cur_iface {
            log::trace!("Current interface state: {cur_iface}");

            if event.ssid.is_none()
                && event.iface_type == InterfaceType::WifiPhy
                && let Interface::WifiPhy(cur_wifi_iface) = cur_iface
            {
                event.ssid = cur_wifi_iface.ssid().map(|s| s.to_string());
            }
        }

        let mut desired_state = NetworkState::default();

        // Purge IP if WIFI PHY interface is down
        if !event.is_up && event.iface_type == InterfaceType::WifiPhy {
            let mut desired_iface = BaseInterface::new(
                event.iface_name.to_string(),
                event.iface_type.clone(),
            );
            desired_iface.state = InterfaceState::Up;
            desired_iface.ipv4 = Some(InterfaceIpv4::new_disabled());
            desired_iface.ipv6 = Some(InterfaceIpv6::new_disabled());
            desired_state.ifaces.push(desired_iface.into());
        }

        for iface in saved_state.ifaces.iter() {
            if is_event_matches(&event, iface, &cur_state.ifaces) {
                let mut desired_iface = iface.clone();
                desired_iface.base_iface_mut().state = InterfaceState::Up;
                if event.is_up {
                    desired_iface.base_iface_mut().up_trigger = None;
                    if event.iface_type == InterfaceType::WifiPhy
                        && iface.iface_type() == &InterfaceType::WifiCfg
                    {
                        desired_iface = wifi_cfg_to_wifi_phy(
                            event.iface_name.as_str(),
                            iface,
                        );
                    }
                } else {
                    if !matches!(
                        iface.base_iface().up_trigger.as_ref(),
                        Some(InterfaceTrigger::Carrier(_))
                    ) && iface.iface_type() != &InterfaceType::WifiCfg
                    {
                        desired_iface.base_iface_mut().state =
                            if iface.is_virtual() {
                                InterfaceState::Absent
                            } else {
                                InterfaceState::Down
                            };
                    }
                    desired_iface.base_iface_mut().down_trigger = None;
                    desired_iface.base_iface_mut().ipv4 =
                        Some(InterfaceIpv4::new_disabled());
                    desired_iface.base_iface_mut().ipv6 =
                        Some(InterfaceIpv6::new_disabled());

                    // WifiCfg bind to any SSID should changed to event
                    // interface only, so other interface is not impacted
                    if let Interface::WifiCfg(saved_iface) = iface
                        && saved_iface.parent().is_none()
                        && let Interface::WifiCfg(desired_iface) =
                            &mut desired_iface
                    {
                        desired_iface.wifi = saved_iface.wifi.clone();
                        if let Some(wifi_cfg) = desired_iface.wifi.as_mut() {
                            wifi_cfg.base_iface =
                                Some(event.iface_name.to_string());
                        }
                    }
                }
                desired_state.ifaces.push(desired_iface);
            }
        }
        if !desired_state.is_empty() {
            log::trace!("Applying desired state due to event {event}");
            log::trace!("Applying desired state {desired_state}");
            let merged_state = MergedNetworkState::new(
                desired_state,
                cur_state,
                NmstateApplyOption::new().no_verify(),
            )?;
            self.apply_merged_state(None, &merged_state).await?;
        } else {
            log::trace!("No change required for event {event}");
        }

        Ok(())
    }
}

/// Event is considered match for specified interface when any of these
/// conditions met:
/// * WiFi NIC found(down) with `Interface::WifiCfg` with bind-to-any SSID.
/// * WiFi event for `Interface::WifiPhy`.
/// * Up event with `up_trigger` matches.
/// * Down event with `down_trigger` matches.
fn is_event_matches(
    event: &NipartLinkEvent,
    saved_iface: &Interface,
    cur_ifaces: &Interfaces,
) -> bool {
    if let Interface::WifiCfg(wifi_iface) = saved_iface
        && event.iface_type == InterfaceType::WifiPhy
    {
        if event.is_up {
            // WIFI connected, we should apply settings on wifi-cfg iface
            // to wifi-phy iface
            if event.ssid.as_deref() == wifi_iface.ssid() {
                return true;
            }
        } else {
            // New WIFI NIC found, we should update WIFI network on it so
            // it could up again when WIFI SSID shows up.
            if let Some(parent) = wifi_iface.parent() {
                if parent == event.iface_name.as_str() {
                    return true;
                }
            } else {
                return true;
            }
        }
    }

    if saved_iface.iface_type() == &InterfaceType::WifiPhy
        && event.iface_type == InterfaceType::WifiPhy
        && event.iface_name.as_str() == saved_iface.name()
    {
        return true;
    }

    if event.is_up
        && let Some(up_trigger) = saved_iface.base_iface().up_trigger.as_ref()
    {
        is_trigger_matches(event, up_trigger, saved_iface, cur_ifaces)
    } else if !event.is_up
        && let Some(down_trigger) =
            saved_iface.base_iface().down_trigger.as_ref()
    {
        is_trigger_matches(event, down_trigger, saved_iface, cur_ifaces)
    } else {
        false
    }
}

fn is_trigger_matches(
    event: &NipartLinkEvent,
    trigger: &InterfaceTrigger,
    saved_iface: &Interface,
    cur_ifaces: &Interfaces,
) -> bool {
    match trigger {
        InterfaceTrigger::Never | InterfaceTrigger::Always => false,
        InterfaceTrigger::Carrier(_) => {
            saved_iface.name() == event.iface_name.as_str()
                && saved_iface.iface_type() == &event.iface_type
        }
        InterfaceTrigger::WifiUp(ssid) => {
            event.is_up && event.ssid.as_deref() == Some(ssid.as_str())
        }
        InterfaceTrigger::WifiDown(ssid) => cur_ifaces
            .iter()
            .filter_map(|i| {
                if let Interface::WifiPhy(i) = i {
                    Some(i)
                } else {
                    None
                }
            })
            .all(|i| i.ssid() != Some(ssid)),
        InterfaceTrigger::WifiUpNot(ssid) => {
            event.is_up && event.ssid.as_deref() != Some(ssid.as_str())
        }
        _ => {
            log::error!(
                "BUG: is_trigger_matches(): unexpected InterfaceTrigger: \
                 {trigger} for event {event}"
            );
            false
        }
    }
}

fn wifi_cfg_to_wifi_phy(
    iface_name: &str,
    saved_iface: &Interface,
) -> Interface {
    let mut desired = saved_iface.base_iface().clone();
    desired.name = iface_name.to_string();
    desired.iface_type = InterfaceType::WifiPhy;

    desired.into()
}
