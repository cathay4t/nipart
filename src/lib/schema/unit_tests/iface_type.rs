// SPDX-License-Identifier: Apache-2.0

use crate::InterfaceType;

#[test]
fn test_iface_type_de_kebab_case_names() {
    for (yaml_str, expected) in [
        ("bond", InterfaceType::Bond),
        ("linux-bridge", InterfaceType::LinuxBridge),
        ("dummy", InterfaceType::Dummy),
        ("ethernet", InterfaceType::Ethernet),
        ("hsr", InterfaceType::Hsr),
        ("loopback", InterfaceType::Loopback),
        ("mac-vlan", InterfaceType::MacVlan),
        ("mac-vtap", InterfaceType::MacVtap),
        ("ovs-bridge", InterfaceType::OvsBridge),
        ("ovs-interface", InterfaceType::OvsInterface),
        ("veth", InterfaceType::Veth),
        ("vlan", InterfaceType::Vlan),
        ("vrf", InterfaceType::Vrf),
        ("vxlan", InterfaceType::Vxlan),
        ("infiniband", InterfaceType::InfiniBand),
        ("tun", InterfaceType::Tun),
        ("macsec", InterfaceType::MacSec),
        ("ipsec", InterfaceType::Ipsec),
        ("xfrm", InterfaceType::Xfrm),
        ("ipvlan", InterfaceType::IpVlan),
        ("wifi-phy", InterfaceType::WifiPhy),
        ("wifi-cfg", InterfaceType::WifiCfg),
        ("wireguard", InterfaceType::Wireguard),
    ] {
        let t: InterfaceType = serde_yaml::from_str(yaml_str).unwrap();
        assert_eq!(t, expected, "type string {yaml_str}");
    }
}

#[test]
fn test_iface_type_de_unknown_string() {
    let t: InterfaceType = serde_yaml::from_str("bogus-type").unwrap();
    assert_eq!(t, InterfaceType::Unknown("bogus-type".to_string()));
    assert!(t.is_unknown());
}

#[test]
fn test_iface_type_serialize_round_trip() {
    assert_eq!(
        serde_yaml::to_value(&InterfaceType::LinuxBridge).unwrap(),
        serde_yaml::Value::String("linux-bridge".to_string())
    );
    assert_eq!(
        serde_yaml::to_value(&InterfaceType::MacSec).unwrap(),
        serde_yaml::Value::String("macsec".to_string())
    );
    assert_eq!(
        serde_yaml::to_value(InterfaceType::Unknown("bogus-type".to_string()))
            .unwrap(),
        serde_yaml::Value::String("bogus-type".to_string())
    );

    for t in [
        InterfaceType::LinuxBridge,
        InterfaceType::WifiPhy,
        InterfaceType::Unknown("bogus-type".to_string()),
    ] {
        let yaml_str = serde_yaml::to_string(&t).unwrap();
        let parsed: InterfaceType = serde_yaml::from_str(&yaml_str).unwrap();
        assert_eq!(parsed, t);
    }
}
