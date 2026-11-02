// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nispor project
// (https://github.com/nispor/nispor) which is under license of Apache 2.0.

use std::collections::HashMap;

use ethtool::{
    EthtoolAttr, EthtoolHeader, EthtoolLinkModeAttr, EthtoolLinkModeDuplex,
};
use futures_util::StreamExt;

use crate::{EthernetConfig, EthernetDuplex};

pub(crate) fn parse_link_mode_message(
    nlas: &[EthtoolAttr],
) -> Option<(String, EthernetConfig)> {
    let mut iface_name = None;
    let mut link_mode = None::<EthernetConfig>;
    for nla in nlas {
        let EthtoolAttr::LinkMode(nla) = nla else {
            continue;
        };
        let config = link_mode.get_or_insert_with(EthernetConfig::default);
        match nla {
            EthtoolLinkModeAttr::Header(headers) => {
                iface_name = headers.iter().find_map(|header| {
                    if let EthtoolHeader::DevName(name) = header {
                        Some(name.to_string())
                    } else {
                        None
                    }
                });
            }
            EthtoolLinkModeAttr::Autoneg(value) => {
                config.auto_neg = Some(*value);
            }
            EthtoolLinkModeAttr::Speed(value) => {
                if *value != u32::MAX {
                    config.speed = Some(*value);
                }
            }
            EthtoolLinkModeAttr::Duplex(value) => {
                config.duplex = match value {
                    EthtoolLinkModeDuplex::Full => Some(EthernetDuplex::Full),
                    EthtoolLinkModeDuplex::Half => Some(EthernetDuplex::Half),
                    _ => None,
                };
            }
            _ => (),
        }
    }
    Some((iface_name?, link_mode?))
}

pub(crate) async fn get_ethtool_link_modes() -> HashMap<String, EthernetConfig>
{
    let mut ret = HashMap::new();
    let (connection, mut handle, _) = match ethtool::new_connection() {
        Ok(value) => value,
        Err(error) => {
            log::warn!("Failed to create ethtool netlink connection: {error}");
            return ret;
        }
    };
    tokio::spawn(connection);
    let mut stream = match handle.link_mode().get(None).execute().await {
        Ok(stream) => stream,
        Err(error) => {
            log::warn!("Failed to query ethtool link mode: {error}");
            return ret;
        }
    };
    while let Some(genl_message) = stream.next().await {
        let message = match genl_message {
            Ok(message) => message,
            Err(error) => {
                log::warn!("Failed to parse ethtool link mode reply: {error}");
                continue;
            }
        };
        if let Some((name, config)) =
            parse_link_mode_message(message.payload.nlas.as_slice())
        {
            ret.insert(name, config);
        }
    }
    ret
}
