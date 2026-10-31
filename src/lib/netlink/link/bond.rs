// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nispor project
// (https://github.com/nispor/nispor) which is under license of Apache 2.0.

use rtnetlink::packet_route::link::{
    BondAdSelect as NlBondAdSelect, BondAllPortActive as NlBondAllPortActive,
    BondArpAllTargets as NlBondArpAllTargets,
    BondArpValidate as NlBondArpValidate, BondFailOverMac as NlBondFailOverMac,
    BondLacpRate as NlBondLacpRate, BondMode as NlBondMode,
    BondPrimaryReselect as NlBondPrimaryReselect,
    BondXmitHashPolicy as NlBondXmitHashPolicy, InfoBond, InfoBondPort,
    InfoData,
};

use crate::{
    BondAdSelect, BondAllPortActive, BondArpAllTargets, BondArpValidate,
    BondConfig, BondFailOverMac, BondLacpRate, BondMode, BondOptions,
    BondPortConfig, BondPrimaryReselect, BondXmitHashPolicy,
    netlink::mac::format_mac_address,
};

#[derive(Debug, Default)]
pub(crate) struct ParsedBondPort {
    pub(crate) config: BondPortConfig,
    pub(crate) permanent_mac: Option<String>,
}

