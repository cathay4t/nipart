// SPDX-License-Identifier: Apache-2.0

use rtnetlink::packet_route::link::{
    BondAdSelect as NlBondAdSelect, BondArpValidate as NlBondArpValidate,
    BondMode as NlBondMode,
};

use crate::{BondAdSelect, BondArpValidate, BondMode};

#[test]
fn test_bond_mode_conversion() {
    let mode: NlBondMode = BondMode::LACP.into();
    assert_eq!(mode, NlBondMode::Ieee8023Ad);
}

#[test]
fn test_bond_arp_validate_conversion() {
    let value: NlBondArpValidate = BondArpValidate::FilterActive.into();
    assert_eq!(value, NlBondArpValidate::FilterActive);
}

#[test]
fn test_bond_ad_select_conversion() {
    let value: NlBondAdSelect = BondAdSelect::ActorPortPrio.into();
    assert_eq!(value, NlBondAdSelect::ActorPortPrio);
}
