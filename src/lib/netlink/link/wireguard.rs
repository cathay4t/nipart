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

fn last_handshake_to_human_str(last_handshake: Duration) -> Option<String> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?;
    if last_handshake > now {
        None
    } else {
        Some(format!("{} seconds ago", (now - last_handshake).as_secs()))
    }
}

impl From<&WireguardConfig> for nl_wireguard::WireguardParsed {
    fn from(config: &WireguardConfig) -> Self {
        let mut parsed = Self::default();
        parsed.private_key = config.private_key.clone();
        parsed.listen_port = config.listen_port;
        parsed.fwmark = config.fwmark;
        parsed.peers = config
            .peers
            .as_ref()
            .map(|peers| peers.iter().map(Into::into).collect());
        parsed
    }
}

impl From<&WireguardPeerConfig> for nl_wireguard::WireguardPeerParsed {
    fn from(peer: &WireguardPeerConfig) -> Self {
        let mut parsed = Self::default();
        parsed.endpoint = peer.endpoint;
        parsed.public_key = peer.public_key.clone();
        parsed.preshared_key = peer.preshared_key.clone();
        parsed.persistent_keepalive = peer.persistent_keepalive;
        parsed.protocol_version = peer.protocol_version;
        parsed.allowed_ips = peer.allowed_ips.as_ref().map(|ips| {
            ips.iter()
                .map(|ip| nl_wireguard::WireguardIpAddress {
                    ip_addr: ip.ip,
                    prefix_length: ip.prefix_length,
                    flags: None,
                })
                .collect()
        });
        parsed
    }
}

pub(crate) async fn apply_wireguard_config(
    iface_name: &str,
    config: &WireguardConfig,
) -> Result<(), crate::NipartError> {
    let (connection, mut handle, _) =
        nl_wireguard::new_connection().map_err(|e| {
            crate::NipartError::new(
                crate::ErrorKind::Bug,
                format!("Failed to create wireguard netlink connection: {e}"),
            )
        })?;
    tokio::spawn(connection);
    let mut parsed: nl_wireguard::WireguardParsed = config.into();
    parsed.iface_name = Some(iface_name.to_string());
    log::trace!("Applying wireguard configuration for interface {iface_name}");
    handle.set(parsed).await.map_err(|e| {
        crate::NipartError::new(
            crate::ErrorKind::Bug,
            format!(
                "Failed to apply wireguard configuration for interface \
                 {iface_name}: {e}"
            ),
        )
    })
}
