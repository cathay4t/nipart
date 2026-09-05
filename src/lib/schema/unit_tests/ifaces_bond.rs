// SPDX-License-Identifier: Apache-2.0

use serde_json::json;

use super::*;

fn deserialize_ad_select(v: serde_json::Value) -> Option<BondAdSelect> {
    let opts: BondOptions = serde_json::from_value(json!({
        "ad_select": v,
    }))
    .unwrap();
    opts.ad_select
}

#[test]
fn test_bond_ad_select_deserialize_strings() {
    assert_eq!(
        deserialize_ad_select(json!("stable")),
        Some(BondAdSelect::Stable)
    );
    assert_eq!(
        deserialize_ad_select(json!("bandwidth")),
        Some(BondAdSelect::Bandwidth)
    );
    assert_eq!(
        deserialize_ad_select(json!("count")),
        Some(BondAdSelect::Count)
    );
    assert_eq!(
        deserialize_ad_select(json!("actor_port_prio")),
        Some(BondAdSelect::ActorPortPrio)
    );
}

#[test]
fn test_bond_ad_select_deserialize_integers() {
    assert_eq!(deserialize_ad_select(json!(0)), Some(BondAdSelect::Stable));
    assert_eq!(
        deserialize_ad_select(json!(1)),
        Some(BondAdSelect::Bandwidth)
    );
    assert_eq!(deserialize_ad_select(json!(2)), Some(BondAdSelect::Count));
    assert_eq!(
        deserialize_ad_select(json!(3)),
        Some(BondAdSelect::ActorPortPrio)
    );
}

#[test]
fn test_bond_ad_select_serialize() {
    let opts = BondOptions {
        ad_select: Some(BondAdSelect::ActorPortPrio),
        ..Default::default()
    };
    let json_str = serde_json::to_string(&opts).unwrap();
    assert_eq!(json_str, "{\"ad_select\":\"actor_port_prio\"}",);
}

#[test]
fn test_bond_ad_select_yaml_round_trip() {
    for (value, expected) in [
        ("stable", BondAdSelect::Stable),
        ("bandwidth", BondAdSelect::Bandwidth),
        ("count", BondAdSelect::Count),
        ("actor_port_prio", BondAdSelect::ActorPortPrio),
    ] {
        let opts: BondOptions =
            rmsd_yaml::from_str(&format!("ad_select: {value}\n")).unwrap();
        assert_eq!(opts.ad_select, Some(expected));
        let serialized = rmsd_yaml::to_string(&opts).unwrap();
        assert_eq!(serialized, format!("ad_select: {value}\n"));
    }
}

#[test]
fn test_bond_ad_actor_system_valid_unicast_mac() {
    let opts = BondOptions {
        ad_actor_system: Some("02:00:00:00:00:03".to_string()),
        ..Default::default()
    };
    assert!(opts.validate_ad_actor_system_mac_address().is_ok());
}

#[test]
fn test_bond_ad_actor_system_reject_iana_multicast_mac() {
    let opts = BondOptions {
        ad_actor_system: Some("01:00:5e:00:00:01".to_string()),
        ..Default::default()
    };
    assert!(opts.validate_ad_actor_system_mac_address().is_err());
}

#[test]
fn test_bond_ad_actor_system_reject_other_multicast_mac() {
    // Kernel rejects any multicast MAC address via
    // is_multicast_ether_addr(), not only the 01:00:5E prefix.
    let opts = BondOptions {
        ad_actor_system: Some("03:00:00:00:00:01".to_string()),
        ..Default::default()
    };
    assert!(opts.validate_ad_actor_system_mac_address().is_err());
}

#[test]
fn test_bond_ad_actor_system_reject_invalid_mac() {
    for mac in [
        "02:00:00",             // too short
        "gg:11:22:33:44:55",    // non hex
        "02:00:00:00:00:03:66", // too long
        "001:11:22:33:44:55",   // more than 2 hex digits per byte
        "0:11:22:33:44:55",     // less than 2 hex digits per byte
    ] {
        let opts = BondOptions {
            ad_actor_system: Some(mac.to_string()),
            ..Default::default()
        };
        assert!(
            opts.validate_ad_actor_system_mac_address().is_err(),
            "expect {mac} to be rejected"
        );
    }
}

#[test]
fn test_bond_ad_actor_system_undefined_is_ok() {
    let opts = BondOptions::default();
    assert!(opts.validate_ad_actor_system_mac_address().is_ok());
}
