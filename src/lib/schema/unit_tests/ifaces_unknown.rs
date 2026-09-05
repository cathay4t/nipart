// SPDX-License-Identifier: Apache-2.0

use super::UnknownInterface;
use crate::{BaseInterface, InterfaceLinkState, InterfaceState, InterfaceType};

#[test]
fn test_deserialize_preserves_query_only_base_fields() {
    let base = BaseInterface {
        name: "vnet0".to_string(),
        iface_type: InterfaceType::Tun,
        state: InterfaceState::Ignore,
        iface_index: Some(6),
        mtu: Some(1500),
        mac_address: Some("FE:54:00:D9:4F:3E".to_string()),
        controller: Some("virbr0".to_string()),
        link_state: Some(InterfaceLinkState::Unknown),
        ..Default::default()
    };
    let iface = UnknownInterface::new(base);
    let value = serde_json::to_value(&iface).unwrap();

    let roundtrip: UnknownInterface = serde_json::from_value(value).unwrap();
    assert_eq!(roundtrip.base.iface_index, Some(6));
    assert_eq!(roundtrip.base.mtu, Some(1500));
    assert_eq!(roundtrip.base.controller.as_deref(), Some("virbr0"));
    assert_eq!(roundtrip.base.link_state, Some(InterfaceLinkState::Unknown));
}
