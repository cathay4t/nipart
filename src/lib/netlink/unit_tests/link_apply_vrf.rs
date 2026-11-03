// SPDX-License-Identifier: Apache-2.0

use rtnetlink::packet_route::link::{InfoData, InfoVrf, LinkMessage};

use super::link_apply::{info_data, vrf_iface};
use crate::netlink::link::vrf;

fn has_vrf_table(message: &LinkMessage) -> bool {
    matches!(info_data(message), Some(InfoData::Vrf(infos))
        if infos.iter().any(|info| matches!(info, InfoVrf::TableId(_))))
}

#[test]
fn test_vrf_builder_new_interface() {
    let message = vrf::vrf_link_builder(&vrf_iface(Some(10)), None)
        .expect("vrf builder should succeed")
        .build();
    assert!(has_vrf_table(&message));
}

#[test]
fn test_vrf_builder_existing_interface_skips_table() {
    let message =
        vrf::vrf_link_builder(&vrf_iface(Some(10)), Some(&vrf_iface(Some(10))))
            .expect("vrf builder should succeed")
            .build();
    assert!(!has_vrf_table(&message));
}

#[test]
fn test_vrf_builder_rejects_table_change() {
    let error =
        vrf::vrf_link_builder(&vrf_iface(Some(20)), Some(&vrf_iface(Some(10))))
            .expect_err("vrf table change should be rejected");
    assert!(error.msg().contains("delete and recreate"));
}

#[test]
fn test_vrf_builder_requires_table_on_create() {
    let error = vrf::vrf_link_builder(&vrf_iface(None), None)
        .expect_err("vrf creation without table ID should be rejected");
    assert!(error.msg().contains("table ID"));
}
