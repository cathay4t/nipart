// SPDX-License-Identifier: Apache-2.0

use serde_json::json;

use super::*;

#[test]
fn test_bond_ad_select_actor_port_prio_to_nispor() {
    let np_ad_select: nispor::BondAdSelect = BondAdSelect::ActorPortPrio.into();
    assert_eq!(np_ad_select, nispor::BondAdSelect::Other(3));
}

#[test]
fn test_bond_ad_select_from_nispor_actor_port_prio() {
    let np_bond: nispor::BondInfo = serde_json::from_value(json!({
        "mode": "802.3ad",
        "ports": [],
        "ad-select": { "other": 3 },
    }))
    .unwrap();
    let opts = np_bond_options_to_nipart(&np_bond);
    assert_eq!(opts.ad_select, Some(BondAdSelect::ActorPortPrio));
}
