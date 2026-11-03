// SPDX-License-Identifier: Apache-2.0

use rtnetlink::packet_route::link::VlanProtocol as NlVlanProtocol;

use crate::{VlanProtocol, VlanQosMapping};

#[test]
fn test_vlan_protocol_conversion() {
    let protocol: NlVlanProtocol = VlanProtocol::Ieee8021Ad.into();
    assert_eq!(protocol, NlVlanProtocol::Ieee8021Ad);
    let protocol: NlVlanProtocol = VlanProtocol::Ieee8021Q.into();
    assert_eq!(protocol, NlVlanProtocol::Ieee8021Q);
    let protocol: NlVlanProtocol = VlanProtocol::Unknown.into();
    assert_eq!(protocol, NlVlanProtocol::Ieee8021Q);
}

#[test]
fn test_vlan_qos_mapping_field_names() {
    let mapping = VlanQosMapping { from: 1, to: 2 };
    assert_eq!(mapping.from, 1);
    assert_eq!(mapping.to, 2);
}
