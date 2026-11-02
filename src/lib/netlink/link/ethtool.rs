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

