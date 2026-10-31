// SPDX-License-Identifier: Apache-2.0

use crate::{
    NipartError, WireguardConfig, WireguardInterface, WireguardIpAddress,
    WireguardPeerConfig,
};

impl From<&WireguardConfig> for nispor::WireguardConf {
    fn from(v: &WireguardConfig) -> Self {
        let mut np_wg = Self::default();
        np_wg.private_key = v.private_key.clone();
        np_wg.listen_port = v.listen_port;
        np_wg.fwmark = v.fwmark;
        np_wg.peers = v
            .peers
            .as_ref()
            .map(|peers| peers.iter().map(|peer| peer.into()).collect());
        np_wg
    }
}

impl From<&WireguardPeerConfig> for nispor::WireguardPeerConf {
    fn from(v: &WireguardPeerConfig) -> Self {
        let mut np_wg = Self::default();
        np_wg.endpoint = v.endpoint;
        np_wg.public_key = v.public_key.clone();
        np_wg.preshared_key = v.preshared_key.clone();
        np_wg.persistent_keepalive = v.persistent_keepalive;
        np_wg.allowed_ips = v
            .allowed_ips
            .as_ref()
            .map(|ips| ips.iter().map(|ip| ip.into()).collect());
        np_wg
    }
}

impl From<&WireguardIpAddress> for nispor::WireguardIpAddress {
    fn from(v: &WireguardIpAddress) -> Self {
        Self {
            address: v.ip,
            prefix_length: v.prefix_length,
        }
    }
}

pub(crate) fn apply_wg_conf(
    mut np_iface: nispor::IfaceConf,
    iface: &WireguardInterface,
) -> Result<Vec<nispor::IfaceConf>, NipartError> {
    if let Some(wg_conf) = iface.wireguard.as_ref() {
        np_iface.wireguard = Some(wg_conf.into());
    }
    Ok(vec![np_iface])
}
