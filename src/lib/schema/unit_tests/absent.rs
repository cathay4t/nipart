// SPDX-License-Identifier: Apache-2.0

use crate::{InterfaceState, Interfaces, MergedInterfaces, NipartInterface};

fn test_absent_to_down_for_type(
    iface_type: &str,
    is_virtual: bool,
    expected_state: InterfaceState,
) {
    let desired: Interfaces = rmsd_yaml::from_str(&format!(
        r#"---
        - name: test0
          type: {iface_type}
          state: absent
        "#
    ))
    .unwrap();

    let current: Interfaces = rmsd_yaml::from_str(&format!(
        r#"---
        - name: test0
          type: {iface_type}
          state: up
        "#
    ))
    .unwrap();

    let merged = MergedInterfaces::new(desired, current, None).unwrap();
    let merged_iface = merged.kernel_ifaces.get("test0").unwrap();
    let for_apply = merged_iface.for_apply.as_ref().unwrap();
    assert_eq!(
        for_apply.base_iface().state,
        expected_state,
        "Interface type {iface_type} (is_virtual={is_virtual}) should have \
         state {:?} in for_apply, got {:?}",
        expected_state,
        for_apply.base_iface().state
    );
}

#[test]
fn test_absent_to_down_non_virtual_ethernet() {
    test_absent_to_down_for_type("ethernet", false, InterfaceState::Down);
}

#[test]
fn test_absent_to_down_non_virtual_wifi_phy() {
    test_absent_to_down_for_type("wifi-phy", false, InterfaceState::Down);
}

#[test]
fn test_absent_stays_absent_virtual_bond() {
    test_absent_to_down_for_type("bond", true, InterfaceState::Absent);
}

#[test]
fn test_absent_stays_absent_virtual_dummy() {
    test_absent_to_down_for_type("dummy", true, InterfaceState::Absent);
}

#[test]
fn test_absent_stays_absent_virtual_linux_bridge() {
    test_absent_to_down_for_type("linux-bridge", true, InterfaceState::Absent);
}

#[test]
fn test_absent_stays_absent_virtual_vlan() {
    test_absent_to_down_for_type("vlan", true, InterfaceState::Absent);
}

#[test]
fn test_absent_stays_absent_virtual_vxlan() {
    test_absent_to_down_for_type("vxlan", true, InterfaceState::Absent);
}

#[test]
fn test_absent_stays_absent_virtual_wireguard() {
    test_absent_to_down_for_type("wireguard", true, InterfaceState::Absent);
}

#[test]
fn test_absent_wireguard_without_current_iface_removes_saved_config() {
    // Desired interface was deleted by other tools, the apply action is just
    // to remove the saved config. This should not error out with
    // "Need wireguard section for creating wireguard interface".
    let desired: Interfaces = rmsd_yaml::from_str(
        r#"---
        - name: test0
          type: wireguard
          state: absent
        "#,
    )
    .unwrap();

    let merged =
        MergedInterfaces::new(desired, Interfaces::default(), None).unwrap();
    let merged_iface = merged.kernel_ifaces.get("test0").unwrap();
    assert!(merged_iface.for_apply.is_some());
    assert!(merged_iface.for_apply.as_ref().unwrap().is_absent());
}
