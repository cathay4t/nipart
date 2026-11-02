// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nispor project
// (https://github.com/nispor/nispor) which is under license of Apache 2.0.

use std::{
    collections::HashMap,
    time::{Duration, SystemTime},
};

use crate::{WireguardConfig, WireguardIpAddress, WireguardPeerConfig};

pub(crate) fn config_from_parsed(
    parsed: nl_wireguard::WireguardParsed,
) -> WireguardConfig {
    WireguardConfig {
        public_key: parsed.public_key,
        private_key: parsed.private_key,
        listen_port: parsed.listen_port,
        fwmark: parsed.fwmark,
        peers: parsed.peers.map(|peers| {
            peers.into_iter().map(peer_config_from_parsed).collect()
        }),
    }
}

fn peer_config_from_parsed(
    peer: nl_wireguard::WireguardPeerParsed,
) -> WireguardPeerConfig {
    WireguardPeerConfig {
        endpoint: peer.endpoint,
        public_key: peer.public_key,
        preshared_key: peer.preshared_key,
        last_handshake: peer
            .last_handshake
            .and_then(last_handshake_to_human_str),
        rx_bytes: peer.rx_bytes,
        tx_bytes: peer.tx_bytes,
        persistent_keepalive: peer.persistent_keepalive,
        allowed_ips: peer.allowed_ips.map(|ips| {
            ips.into_iter()
                .map(|ip| WireguardIpAddress {
                    ip: ip.ip_addr,
                    prefix_length: ip.prefix_length,
                })
                .collect()
        }),
        protocol_version: peer.protocol_version,
    }
}

pub(crate) async fn get_wireguard_configs(
    iface_names: &[String],
) -> HashMap<String, WireguardConfig> {
    let mut ret = HashMap::new();
    if iface_names.is_empty() {
        return ret;
    }
    let (connection, mut handle, _) = match nl_wireguard::new_connection() {
        Ok(value) => value,
        Err(error) => {
            log::warn!(
                "Failed to create wireguard netlink connection: {error}"
            );
            return ret;
        }
    };
    tokio::spawn(connection);
    for iface_name in iface_names {
        match handle.get_by_name(iface_name.as_str()).await {
            Ok(parsed) => {
                ret.insert(iface_name.clone(), config_from_parsed(parsed));
            }
            Err(error) => {
                // Mostly permission error because querying wireguard info
                // needs root access.
                log::debug!(
                    "Failed to query wireguard information of {iface_name}: \
                     {error}"
                );
            }
        }
    }
    ret
}

