// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn test_bond_ad_select_actor_port_prio_to_nispor() {
    let np_ad_select: nispor::BondAdSelect = BondAdSelect::ActorPortPrio.into();
    assert_eq!(np_ad_select, nispor::BondAdSelect::Other(3));
}
