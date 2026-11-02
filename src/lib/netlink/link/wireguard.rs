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

