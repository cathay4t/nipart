// SPDX-License-Identifier: Apache-2.0

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::{Duration, SystemTime},
};

use crate::{WireguardIpAddress, netlink::link::wireguard::config_from_parsed};

fn parsed_wireguard() -> nl_wireguard::WireguardParsed {
    let mut peer = nl_wireguard::WireguardPeerParsed::default();
    peer.endpoint = Some(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
        51820,
    ));
    peer.public_key = Some("peer-public-key".to_string());
    peer.preshared_key = Some("peer-preshared-secret".to_string());
    peer.persistent_keepalive = Some(25);
    peer.last_handshake = Some(
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            - Duration::from_secs(32),
    );
    peer.rx_bytes = Some(1024);
    peer.tx_bytes = Some(2048);
    peer.allowed_ips = Some(vec![nl_wireguard::WireguardIpAddress {
        ip_addr: IpAddr::V4(Ipv4Addr::new(192, 0, 2, 0)),
        prefix_length: 24,
        flags: None,
    }]);
    peer.protocol_version = Some(1);

    let mut parsed = nl_wireguard::WireguardParsed::default();
    parsed.public_key = Some("public-key".to_string());
    parsed.private_key = Some("wg-private-secret".to_string());
    parsed.listen_port = Some(51820);
    parsed.fwmark = Some(100);
    parsed.peers = Some(vec![peer]);
    parsed
}

