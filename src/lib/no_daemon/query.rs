// SPDX-License-Identifier: Apache-2.0

use super::{route::get_routes, route_rule::get_route_rules};
use crate::{
    ErrorKind, Interfaces, NetworkState, NipartError, NipartInterface,
    NipartNoDaemon, NipartQueryOption,
    netlink::{
        addr::{
            get_address_messages, ipv4_map_from_messages,
            ipv6_map_from_messages,
        },
        link::{LinkState, get_link_messages},
    },
};

impl NipartNoDaemon {
    pub async fn query_network_state(
        option: NipartQueryOption,
    ) -> Result<NetworkState, NipartError> {
        if option.version != 1 {
            return Err(NipartError::new(
                ErrorKind::InvalidSchemaVersion,
                format!(
                    "Only support version 1, but desired {}",
                    option.version
                ),
            ));
        }
        // TODO: check other property in NipartQueryOption

        let mut net_state = NetworkState::default();
        let link_messages = get_link_messages().await?;
        let mut link_state = LinkState::from_messages(&link_messages);

        let address_messages = match get_address_messages().await {
            Ok(messages) => messages,
            Err(e) => {
                log::warn!(
                    "Failed to retrieve addresses via netlink, falling back \
                     to disabled IP state: {e}"
                );
                Vec::new()
            }
        };
        let mut ipv4_map = ipv4_map_from_messages(&address_messages);
        let mut ipv6_map = ipv6_map_from_messages(&address_messages);

        let mut ifaces = Interfaces::new(link_state.into_interfaces());
        for iface in ifaces.iter_mut() {
            let base_iface = iface.base_iface_mut();
            let iface_index = base_iface.iface_index.unwrap_or_default();
            base_iface.ipv4 =
                Some(ipv4_map.remove(&iface_index).unwrap_or_default());
            base_iface.ipv6 =
                Some(ipv6_map.remove(&iface_index).unwrap_or_default());
        }
        net_state.ifaces = ifaces;

        net_state.routes = get_routes(&net_state.ifaces).await;
        net_state.route_rules = get_route_rules().await?;
        net_state.dns_resolver = super::resolv_conf::query_running_dns()?;

        net_state
            .routes
            .mark_route_as_ignored_ifaces(&net_state.ifaces);
        Ok(net_state)
    }
}
