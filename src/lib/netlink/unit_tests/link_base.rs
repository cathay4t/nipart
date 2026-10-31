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

