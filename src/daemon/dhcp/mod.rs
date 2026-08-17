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

const WIFI_SSID_WAIT_TIMEOUT_SECS: u64 = 60;

/// Wait until the wifi-phy reports the desired SSID.
///
/// Used when an apply switches a wifi-phy to a different SSID: DHCP must
/// not start until the new association is up, otherwise the client can
/// still receive a lease from the old network.
pub(crate) async fn wait_wifi_ssid(
    iface_name: &str,
    ssid: &str,
) -> Result<(), NipartError> {
    let deadline = std::time::Instant::now()
        + Duration::from_secs(WIFI_SSID_WAIT_TIMEOUT_SECS);
    loop {
        let state =
            NipartNoDaemon::query_network_state(NipartQueryOption::running())
                .await?;
        if let Some(iface) = state.ifaces.kernel_ifaces.get(iface_name)
            && let Interface::WifiPhy(wifi_iface) = iface
            && wifi_iface.ssid() == Some(ssid)
        {
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
