// SPDX-License-Identifier: Apache-2.0

mod dhcp_manager;
mod dhcp_worker;
mod dhcpv6_manager;
mod dhcpv6_worker;

use std::time::Duration;

use nipart::{
    ErrorKind, Interface, NipartError, NipartNoDaemon, NipartQueryOption,
};

pub(crate) use self::{
    dhcp_manager::NipartDhcpV4Manager,
    dhcp_worker::{NipartDhcpCmd, NipartDhcpReply, NipartDhcpV4Worker},
    dhcpv6_manager::NipartDhcpV6Manager,
    dhcpv6_worker::{NipartDhcpV6Cmd, NipartDhcpV6Reply, NipartDhcpV6Worker},
};
use crate::plugin::NipartPluginManager;

const WIFI_SSID_WAIT_TIMEOUT_SECS: u64 = 60;

/// Whether the apply changed the SSID of a wifi-phy.
///
/// The link-up event path synthesizes a wifi-phy from a saved wifi-cfg to
/// carry the IP config; that synthetic interface intentionally has no
/// `wifi` section.  Without a desired SSID we cannot claim an SSID change,
/// otherwise every repeated up event would tear down a healthy DHCP lease.
pub(crate) fn wifi_ssid_changed(
    current: Option<&Interface>,
    desired: Option<&Interface>,
) -> bool {
    match (current, desired) {
        (Some(Interface::WifiPhy(cur)), Some(Interface::WifiPhy(des))) => des
            .ssid()
            .is_some_and(|des_ssid| cur.ssid() != Some(des_ssid)),
        _ => false,
    }
}

/// Whether an apply should touch the DHCP client of an interface.
///
/// A merge diff can be caused by saved-only fields (e.g. `profile-name`)
/// that do not require any DHCP change. `restart_auto_ip` applies and SSID
/// changes still restart DHCP even when the IP diff omitted the unchanged
/// DHCP settings; interfaces going down always need their DHCP client
/// stopped.
pub(crate) fn should_touch_dhcp(
    restart_auto_ip: bool,
    ssid_changed: bool,
    ip_conf_changed: bool,
    iface_is_up: bool,
) -> bool {
    !iface_is_up || restart_auto_ip || ssid_changed || ip_conf_changed
}

/// Wait until the wifi-phy reports the desired SSID.
///
/// Used when an apply switches a wifi-phy to a different SSID: DHCP must
/// not start until the new association is up, otherwise the client can
/// still receive a lease from the old network.
pub(crate) async fn wait_wifi_ssid(
    iface_name: &str,
    ssid: &str,
    plugin_manager: &mut NipartPluginManager,
) -> Result<(), NipartError> {
    let deadline = std::time::Instant::now()
        + Duration::from_secs(WIFI_SSID_WAIT_TIMEOUT_SECS);
    loop {
        let mut state =
            NipartNoDaemon::query_network_state(NipartQueryOption::running())
                .await?;
        if wifi_state_has_ssid(&state, iface_name, ssid) || {
            // mac80211_hwsim and some drivers do not expose the
            // association SSID through nispor in time, while the wifi
            // plugin's shuli client already knows it is connected.
            let plugin_states = plugin_manager
                .query_network_state(NipartQueryOption::running(), &state)
                .await?;
            for plugin_state in plugin_states {
                state.merge(&plugin_state)?;
            }
            wifi_state_has_ssid(&state, iface_name, ssid)
        } {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Err(NipartError::new(
        ErrorKind::Timeout,
        format!(
            "Timed out waiting for wifi SSID {ssid} on interface {iface_name}"
        ),
    ))
}

fn wifi_state_has_ssid(
    state: &nipart::NetworkState,
    iface_name: &str,
    ssid: &str,
) -> bool {
    state
        .ifaces
        .kernel_ifaces
        .get(iface_name)
        .and_then(|iface| {
            if let Interface::WifiPhy(wifi_iface) = iface {
                wifi_iface.ssid()
            } else {
                None
            }
        })
        == Some(ssid)
}

#[cfg(test)]
#[path = "../unit_tests/dhcp.rs"]
mod tests;
