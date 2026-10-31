// SPDX-License-Identifier: Apache-2.0

use rtnetlink::packet_route::link::{
    InfoData, InfoKind, LinkAttribute, LinkFlags, LinkInfo, LinkLayerType,
    Prop, State,
};

use super::fixtures::{link_info_attribute, link_message};
use crate::{
    AltNameEntry, Interface, InterfaceLinkState, InterfaceState, InterfaceType,
    NipartInterface, netlink::link::LinkState,
};

fn interface_by_name(interfaces: Vec<Interface>, name: &str) -> Interface {
    interfaces
        .into_iter()
        .find(|interface| interface.name() == name)
        .unwrap_or_else(|| panic!("Interface {name} not found"))
}

#[test]
fn test_parse_base_link_attributes() {
    let mut message = link_message(
        2,
        "eth1",
        LinkLayerType::Ether,
        LinkFlags::Up | LinkFlags::LowerUp | LinkFlags::Running,
    );
    message.attributes.extend([
        LinkAttribute::Mtu(9000),
        LinkAttribute::MinMtu(68),
        LinkAttribute::MaxMtu(9216),
        LinkAttribute::Address(vec![0x02, 0x00, 0x00, 0x00, 0x01, 0xaa]),
        LinkAttribute::PermAddress(vec![0x02, 0x00, 0x00, 0x00, 0x01, 0xbb]),
        LinkAttribute::OperState(State::Up),
        LinkAttribute::Controller(10),
        LinkAttribute::PropList(vec![
            Prop::AltIfName("zz-alt".to_string()),
            Prop::AltIfName("aa-alt".to_string()),
        ]),
        LinkAttribute::ParentDevName("0000:02:00.0".to_string()),
        LinkAttribute::ParentDevBusName("pci".to_string()),
    ]);
    let controller =
        link_message(10, "br0", LinkLayerType::Ether, LinkFlags::Up);
    let interface = interface_by_name(
        LinkState::from_messages(&[message, controller]).into_interfaces(),
        "eth1",
    );
    let base = interface.base_iface();
    assert_eq!(base.iface_type, InterfaceType::Ethernet);
    assert_eq!(base.iface_index, Some(2));
    assert_eq!(base.state, InterfaceState::Up);
    assert_eq!(base.link_state, Some(InterfaceLinkState::Up));
    assert_eq!(base.mtu, Some(9000));
    assert_eq!(base.min_mtu, Some(68));
    assert_eq!(base.max_mtu, Some(9216));
    assert_eq!(base.mac_address.as_deref(), Some("02:00:00:00:01:AA"));
    assert_eq!(
        base.permanent_mac_address.as_deref(),
        Some("02:00:00:00:01:BB")
    );
    assert_eq!(base.controller.as_deref(), Some("br0"));
    assert_eq!(
        base.alt_names,
        Some(vec![
            AltNameEntry {
                name: "aa-alt".to_string(),
                state: None,
            },
            AltNameEntry {
                name: "zz-alt".to_string(),
                state: None,
            },
        ])
    );
    assert_eq!(base.pci_address.as_deref(), Some("0000:02:00.0"));
}

#[test]
fn test_parse_link_state_defaults() {
    let message =
        link_message(3, "dummy0", LinkLayerType::Ether, LinkFlags::empty());
    let interface = interface_by_name(
        LinkState::from_messages(&[message]).into_interfaces(),
        "dummy0",
    );
    let base = interface.base_iface();
    assert_eq!(base.state, InterfaceState::Unknown);
    assert_eq!(base.link_state, Some(InterfaceLinkState::Unknown));
    assert_eq!(base.mtu, Some(0));
    assert_eq!(base.mac_address.as_deref(), Some(""));
    assert_eq!(base.permanent_mac_address, None);
}

#[test]
fn test_parse_loopback_and_unsupported_link() {
    let loopback =
        link_message(1, "lo", LinkLayerType::Loopback, LinkFlags::Loopback);
    let mut tun = link_message(4, "tun0", LinkLayerType::Ether, LinkFlags::Up);
    tun.attributes.push(link_info_attribute(
        InfoKind::Tun,
        Some(InfoData::Other(Vec::new())),
    ));
    let interfaces =
        LinkState::from_messages(&[loopback, tun]).into_interfaces();
    let loopback = interface_by_name(interfaces.clone(), "lo");
    assert_eq!(loopback.iface_type(), &InterfaceType::Loopback);
    assert_eq!(loopback.base_iface().state, InterfaceState::Unknown);
    let tun = interface_by_name(interfaces, "tun0");
    assert_eq!(tun.base_iface().state, InterfaceState::Ignore);
}

