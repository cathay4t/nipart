// SPDX-License-Identifier: Apache-2.0

use ethtool::{
    EthtoolAttr, EthtoolHeader, EthtoolLinkModeAttr, EthtoolLinkModeDuplex,
};

use crate::{EthernetDuplex, netlink::link::ethtool::parse_link_mode_message};

#[test]
fn test_parse_ethtool_link_mode() {
    let nlas = vec![
        EthtoolAttr::LinkMode(EthtoolLinkModeAttr::Header(vec![
            EthtoolHeader::DevName("eth1".to_string()),
        ])),
        EthtoolAttr::LinkMode(EthtoolLinkModeAttr::Autoneg(true)),
        EthtoolAttr::LinkMode(EthtoolLinkModeAttr::Speed(1000)),
        EthtoolAttr::LinkMode(EthtoolLinkModeAttr::Duplex(
            EthtoolLinkModeDuplex::Full,
        )),
    ];
    let (name, config) =
        parse_link_mode_message(&nlas).expect("Link mode missing");
    assert_eq!(name, "eth1");
    assert_eq!(config.speed, Some(1000));
    assert_eq!(config.auto_neg, Some(true));
    assert_eq!(config.duplex, Some(EthernetDuplex::Full));
}

#[test]
fn test_parse_ethtool_unknown_speed_and_duplex() {
    let nlas = vec![
        EthtoolAttr::LinkMode(EthtoolLinkModeAttr::Header(vec![
            EthtoolHeader::DevName("eth1".to_string()),
        ])),
        EthtoolAttr::LinkMode(EthtoolLinkModeAttr::Speed(u32::MAX)),
        EthtoolAttr::LinkMode(EthtoolLinkModeAttr::Duplex(
            EthtoolLinkModeDuplex::Unknown,
        )),
    ];
    let (_, config) =
        parse_link_mode_message(&nlas).expect("Link mode missing");
    assert_eq!(config.speed, None);
    assert_eq!(config.auto_neg, None);
    assert_eq!(config.duplex, None);
}

#[test]
fn test_parse_ethtool_without_header() {
    let nlas = vec![EthtoolAttr::LinkMode(EthtoolLinkModeAttr::Autoneg(true))];
    assert!(parse_link_mode_message(&nlas).is_none());
}
