// SPDX-License-Identifier: Apache-2.0

use futures_util::stream::{StreamExt, TryStreamExt};
use rtnetlink::{
    MulticastGroup, new_multicast_connection,
    packet_core::NetlinkPayload,
    packet_route::{
        RouteNetlinkMessage,
        link::{LinkAttribute, State},
    },
};

use crate::{ErrorKind, NipartError, NipartNoDaemon};

impl NipartNoDaemon {
    pub async fn wait_link_carrier_up(
        iface_name: &str,
    ) -> Result<(), NipartError> {
        wait_link_carrier(iface_name, true).await
    }

    pub async fn wait_link_carrier_down(
        iface_name: &str,
    ) -> Result<(), NipartError> {
        wait_link_carrier(iface_name, false).await
    }

    /// Enable IPv6 on the interface by setting
    /// `/proc/sys/net/ipv6/conf/<iface>/disable_ipv6` to `0`.
    /// The DHCPv6 client requires the interface to hold an IPv6 link-local
    /// address as the source address of its traffic, so this must be done
    /// before starting DHCPv6 when IPv6 was previously disabled.
    pub async fn enable_ipv6(iface_name: &str) -> Result<(), NipartError> {
        let sysctl_path =
            format!("/proc/sys/net/ipv6/conf/{iface_name}/disable_ipv6");
        tokio::fs::write(&sysctl_path, "0\n").await.map_err(|e| {
            NipartError::new(
                ErrorKind::Bug,
                format!(
                    "Failed to enable IPv6 on interface {iface_name} by \
                         writing {sysctl_path}: {e}"
                ),
            )
        })?;
        Ok(())
    }

    /// Force allow IPv6 router advertisements on the interface by setting
    /// `/proc/sys/net/ipv6/conf/<iface>/accept_ra` to `2`.
    /// This is required for IPv6 autoconf (SLAAC) to run even when IPv6
    /// forwarding is enabled on the interface.
    pub async fn enable_autoconf(iface_name: &str) -> Result<(), NipartError> {
        let sysctl_path =
            format!("/proc/sys/net/ipv6/conf/{iface_name}/accept_ra");
        tokio::fs::write(&sysctl_path, "2\n").await.map_err(|e| {
            NipartError::new(
                ErrorKind::Bug,
                format!(
                    "Failed to enable IPv6 autoconf on interface {iface_name} \
                     by writing {sysctl_path}: {e}"
                ),
            )
        })?;
        Ok(())
    }

    /// When the interface holds no IPv6 link-local address, flip the
    /// `disable_ipv6` sysctl so the kernel regenerates the IPv6 link-local
    /// address for it. No-op when the link-local address already exists.
    pub async fn regenerate_link_local(
        iface_name: &str,
    ) -> Result<(), NipartError> {
        if has_link_local_addr(iface_name).await? {
            return Ok(());
        }
        log::info!(
            "Interface {iface_name} holds no IPv6 link-local address, \
             flipping disable_ipv6 to force the kernel to regenerate it"
        );
        let sysctl_path =
            format!("/proc/sys/net/ipv6/conf/{iface_name}/disable_ipv6");
        for value in ["1", "0"] {
            tokio::fs::write(&sysctl_path, value).await.map_err(|e| {
                NipartError::new(
                    ErrorKind::Bug,
                    format!(
                        "Failed to flip IPv6 disable_ipv6 on interface \
                         {iface_name} by writing {sysctl_path}: {e}"
                    ),
                )
            })?;
        }
        Ok(())
    }
}

async fn has_link_local_addr(iface_name: &str) -> Result<bool, NipartError> {
    let mut filter = nispor::NetStateFilter::minimum();
    let mut iface_filter = nispor::NetStateIfaceFilter::default();
    iface_filter.iface_name = Some(iface_name.to_string());
    filter.iface = Some(iface_filter);
    let np_state =
        nispor::NetState::retrieve_with_filter_async(&filter).await?;
    if let Some(np_iface) = np_state.ifaces.get(iface_name)
        && let Some(np_ip) = np_iface.ipv6.as_ref()
    {
        for np_addr in &np_ip.addresses {
            if let Ok(ip) = np_addr.address.parse::<std::net::Ipv6Addr>()
                && ip.is_unicast_link_local()
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

async fn wait_link_carrier(
    iface_name: &str,
    link_up: bool,
) -> Result<(), NipartError> {
    // netlink multicast socket will be used for one-time query and also follow
    // up monitor
    let (conn, handle, mut messages) =
        new_multicast_connection(&[MulticastGroup::Link]).map_err(|e| {
            NipartError::new(
                ErrorKind::InvalidArgument,
                format!(
                    "Failed to create netlink multicast socket for interface \
                     {iface_name}: {e}"
                ),
            )
        })?;
    tokio::spawn(conn);

    let cur_link_state = is_link_carrier_up(&handle, iface_name).await?;
    if link_up == cur_link_state {
        return Ok(());
    }

    let iface_name_attr = LinkAttribute::IfName(iface_name.to_string());

    while let Some((nl_msg, _)) = messages.next().await {
        if let NetlinkPayload::InnerMessage(RouteNetlinkMessage::NewLink(
            link_msg,
        )) = nl_msg.payload
            && link_msg
                .attributes
                .iter()
                .any(|attr| attr == &iface_name_attr)
            && link_msg.attributes.iter().any(|attr| {
                if link_up {
                    &LinkAttribute::OperState(State::Up) == attr
                } else {
                    &LinkAttribute::OperState(State::Up) != attr
                }
            })
        {
            return Ok(());
        }
    }
    Err(NipartError::new(
        ErrorKind::Bug,
        "wait_link_carrier(): Kernel terminated the netlink multicast socket \
         connection"
            .into(),
    ))
}

async fn is_link_carrier_up(
    handle: &rtnetlink::Handle,
    iface_name: &str,
) -> Result<bool, NipartError> {
    let mut links = handle
        .link()
        .get()
        .match_name(iface_name.to_string())
        .execute();
    while let Some(link_msg) = links.try_next().await.map_err(|e| {
        NipartError::new(
            ErrorKind::Bug,
            format!(
                "Failed to query rtnetlink link subsystem for checking link \
                 carrier of {}: {e}",
                iface_name
            ),
        )
    })? {
        for attr in link_msg.attributes {
            if LinkAttribute::OperState(State::Up) == attr {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
