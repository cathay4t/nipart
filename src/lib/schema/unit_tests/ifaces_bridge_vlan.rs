// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn test_compress_bridge_vlan_ids() {
    let vids = vec![11u16, 1, 2, 3, 4, 9, 10, 20, 25, 26];

    assert_eq!(
        compress_vlan_ids(vids),
        vec![
            BridgeVlanTrunkTag::IdRange(BridgeVlanRange { min: 1, max: 4 }),
            BridgeVlanTrunkTag::IdRange(BridgeVlanRange { min: 9, max: 11 }),
            BridgeVlanTrunkTag::Id(20),
            BridgeVlanTrunkTag::IdRange(BridgeVlanRange { min: 25, max: 26 })
        ],
    )
}
