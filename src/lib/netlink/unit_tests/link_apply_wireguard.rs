// SPDX-License-Identifier: Apache-2.0

use crate::{WireguardConfig, WireguardIpAddress, WireguardPeerConfig};

#[test]
fn test_wireguard_config_conversion() {
    let config = WireguardConfig {
        private_key: Some("private".to_string()),
        listen_port: Some(51820),
        fwmark: Some(42),
        peers: Some(vec![WireguardPeerConfig {
            endpoint: Some("192.0.2.1:51820".parse().unwrap()),
            public_key: Some("public".to_string()),
            preshared_key: Some("preshared".to_string()),
            persistent_keepalive: Some(25),
            allowed_ips: Some(vec![WireguardIpAddress {
                ip: "192.0.2.0".parse().unwrap(),
                prefix_length: 24,
            }]),
            ..Default::default()
        }]),
        ..Default::default()
    };
    let parsed: nl_wireguard::WireguardParsed = (&config).into();
    assert_eq!(parsed.private_key.as_deref(), Some("private"));
    assert_eq!(parsed.listen_port, Some(51820));
    assert_eq!(parsed.fwmark, Some(42));
    let peers = parsed.peers.expect("peers should be converted");
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].persistent_keepalive, Some(25));
    assert_eq!(
        peers[0]
            .allowed_ips
            .as_ref()
            .map(|ips| ips[0].prefix_length),
        Some(24)
    );
}
