// SPDX-License-Identifier: Apache-2.0

use crate::{
    ErrorKind, MergedNetworkState, NetworkState, NipartApplyOption,
    NipartInterface,
};

fn gen_merged(
    desired: &str,
    current: &str,
) -> Result<MergedNetworkState, crate::NipartError> {
    let desired = NetworkState::new_from_yaml(desired).unwrap();
    let current = NetworkState::new_from_yaml(current).unwrap();
    MergedNetworkState::new(
        desired,
        current,
        None,
        NipartApplyOption::default(),
    )
}

fn gen_merged_with_saved(
    desired: &str,
    current: &str,
    saved: &str,
) -> Result<MergedNetworkState, crate::NipartError> {
    let desired = NetworkState::new_from_yaml(desired).unwrap();
    let current = NetworkState::new_from_yaml(current).unwrap();
    let saved = NetworkState::new_from_yaml(saved).unwrap();
    MergedNetworkState::new(
        desired,
        current,
        Some(saved),
        NipartApplyOption::default(),
    )
}

fn assert_invalid_argument(
    result: Result<MergedNetworkState, crate::NipartError>,
    msg_part: &str,
) {
    let e = result.expect_err("Should fail on invalid alt-names");
    assert_eq!(e.kind(), ErrorKind::InvalidArgument);
    assert!(
        e.msg().contains(msg_part),
        "Error message {e} should contain {msg_part}"
    );
}

#[test]
fn test_alt_name_same_as_iface_name_allowed() {
    // The user's example: `name: port1` also holds `alt-names: [port1,
    // primary]`.  Unlike nmstate, this is allowed.
    let result = gen_merged(
        r#"---
        interfaces:
          - name: port1
            type: ethernet
            identifier: mac-address
            mac-address: 52:54:00:15:17:63
            alt-names:
              - name: port1
              - name: primary
        "#,
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            mac-address: 52:54:00:15:17:63
        "#,
    );
    assert!(result.is_ok());
}

#[test]
fn test_alt_names_sorted_in_merged_state() {
    let merged = gen_merged(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            alt-names:
              - name: zebra
              - name: alpha
              - name: mike
        "#,
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
        "#,
    )
    .unwrap();
    let merged_iface = merged.ifaces.kernel_ifaces.get("eth1").unwrap();
    let for_apply = merged_iface.for_apply.as_ref().unwrap();
    let alt_names: Vec<&str> = for_apply
        .base_iface()
        .alt_names
        .as_ref()
        .unwrap()
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(alt_names, vec!["alpha", "mike", "zebra"]);
}

#[test]
fn test_alt_name_conflict_with_other_iface() {
    let result = gen_merged(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            alt-names:
              - name: primary
          - name: eth2
            type: ethernet
            alt-names:
              - name: primary
        "#,
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
          - name: eth2
            type: ethernet
        "#,
    );
    assert_invalid_argument(result, "already used by interface");
}

#[test]
fn test_alt_name_conflict_with_kernel_iface_name() {
    let result = gen_merged(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            alt-names:
              - name: eth2
        "#,
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
          - name: eth2
            type: ethernet
        "#,
    );
    assert_invalid_argument(result, "already an interface name");
}

#[test]
fn test_alt_name_same_as_own_kernel_name_rejected() {
    // An interface cannot hold an alt-name equal to its own kernel name
    // (the kernel rejects it); the error must not claim it belongs to
    // another NIC.
    let result = gen_merged(
        r#"---
        interfaces:
          - name: port1
            type: ethernet
            identifier: mac-address
            mac-address: 52:54:00:15:17:63
            alt-names:
              - name: enp1s0
        "#,
        r#"---
        interfaces:
          - name: enp1s0
            type: ethernet
            mac-address: 52:54:00:15:17:63
        "#,
    );
    let e = result.expect_err("Should reject alt-name == own kernel name");
    assert_eq!(e.kind(), ErrorKind::InvalidArgument);
    assert!(
        e.msg().contains("kernel interface name"),
        "Error message {e} should mention the kernel interface name"
    );
}

#[test]
fn test_alt_name_yaml_round_trip() {
    let yaml = r#"---
        interfaces:
          - name: eth1
            type: ethernet
            alt-names:
              - name: primary
              - name: port1
              - name: old
                state: absent
        "#;
    let state = NetworkState::new_from_yaml(yaml).unwrap();
    let serialized = rmsd_yaml::to_string(&state).unwrap();
    let reparsed = NetworkState::new_from_yaml(&serialized).unwrap();
    assert_eq!(state, reparsed);
}

#[test]
fn test_alt_name_keep_existing_on_mac_identified_apply() {
    // Current eth1 (matched by MAC) already holds alt-name `port1`; the
    // desired `identifier: mac-address` config re-applies with the profile
    // name `port1` and keeps `port1` + adds `primary`.  The existing
    // alt-name is owned by the same physical interface: this must not be
    // reported as a conflict with another interface.
    let result = gen_merged(
        r#"---
        interfaces:
          - name: port1
            type: ethernet
            identifier: mac-address
            mac-address: 52:54:00:15:17:63
            alt-names:
              - name: port1
              - name: primary
        "#,
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            mac-address: 52:54:00:15:17:63
            alt-names:
              - name: port1
        "#,
    );
    assert!(
        result.is_ok(),
        "Should not report self-owned alt-name as conflict: {result:?}"
    );
}

#[test]
fn test_duplicate_alt_name_on_same_iface_rejected() {
    let result = gen_merged(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            alt-names:
              - name: primary
              - name: primary
                state: absent
        "#,
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
        "#,
    );
    assert_invalid_argument(result, "Duplicate alt-name");
}

#[test]
fn test_revert_added_alt_name_generates_absent() {
    // Applying `alt-names: [primary]` to an interface without any: the
    // revert state must mark `primary` as `state: absent` so a rollback
    // removes the added alt-name (the incremental apply only touches
    // listed entries).
    let merged = gen_merged(
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
            alt-names:
              - name: primary
        "#,
        r#"---
        interfaces:
          - name: eth1
            type: ethernet
        "#,
    )
    .unwrap();
    let merged_iface = merged.ifaces.kernel_ifaces.get("eth1").unwrap();
    let revert = merged_iface.for_revert.as_ref().unwrap();
    let alt_names = revert.base_iface().alt_names.as_ref().unwrap();
    assert_eq!(alt_names.len(), 1);
    assert_eq!(alt_names[0].name, "primary");
    assert!(alt_names[0].is_absent());
}

#[test]
fn test_mac_id_kernel_iface_name_rename_keeps_original_as_alt_name() {
    // A MAC-identified config with an explicit `kernel-iface-name` renames
    // the matched interface and keeps the original kernel name as an
    // alt-name (no `alt-names` defined in desired or saved state).
    let merged = gen_merged(
        r#"---
        interfaces:
          - name: port1
            type: ethernet
            identifier: mac-address
            mac-address: 52:54:00:15:17:63
            kernel-iface-name: eth0
        "#,
        r#"---
        interfaces:
          - name: enp1s0
            type: ethernet
            mac-address: 52:54:00:15:17:63
        "#,
    )
    .unwrap();
    let merged_iface = merged.ifaces.kernel_ifaces.get("eth0").unwrap();
    let for_apply = merged_iface.for_apply.as_ref().unwrap();
    assert_eq!(for_apply.kernel_iface_name(), "eth0");
    let alt_names = for_apply.base_iface().alt_names.as_ref().unwrap();
    assert_eq!(alt_names.len(), 1);
    assert_eq!(alt_names[0].name, "enp1s0");
    assert!(!alt_names[0].is_absent());
}

#[test]
fn test_mac_id_kernel_iface_name_no_auto_alt_name_when_desired_defines() {
    // When the desired state manages `alt-names` explicitly, the original
    // kernel name is not auto-added.
    let merged = gen_merged(
        r#"---
        interfaces:
          - name: port1
            type: ethernet
            identifier: mac-address
            mac-address: 52:54:00:15:17:63
            kernel-iface-name: eth0
            alt-names:
              - name: primary
        "#,
        r#"---
        interfaces:
          - name: enp1s0
            type: ethernet
            mac-address: 52:54:00:15:17:63
        "#,
    )
    .unwrap();
    let merged_iface = merged.ifaces.kernel_ifaces.get("eth0").unwrap();
    let for_apply = merged_iface.for_apply.as_ref().unwrap();
    assert_eq!(for_apply.kernel_iface_name(), "eth0");
    let alt_names = for_apply.base_iface().alt_names.as_ref().unwrap();
    assert_eq!(alt_names.len(), 1);
    assert_eq!(alt_names[0].name, "primary");
}

#[test]
fn test_mac_id_kernel_iface_name_no_auto_alt_name_when_saved_defines() {
    // When the saved state manages `alt-names` explicitly, the original
    // kernel name is not auto-added.
    let merged = gen_merged_with_saved(
        r#"---
        interfaces:
          - name: port1
            type: ethernet
            identifier: mac-address
            mac-address: 52:54:00:15:17:63
            kernel-iface-name: eth0
        "#,
        r#"---
        interfaces:
          - name: enp1s0
            type: ethernet
            mac-address: 52:54:00:15:17:63
        "#,
        r#"---
        interfaces:
          - name: port1
            type: ethernet
            identifier: mac-address
            mac-address: 52:54:00:15:17:63
            kernel-iface-name: eth0
            alt-names:
              - name: primary
        "#,
    )
    .unwrap();
    let merged_iface = merged.ifaces.kernel_ifaces.get("eth0").unwrap();
    let for_apply = merged_iface.for_apply.as_ref().unwrap();
    assert_eq!(for_apply.kernel_iface_name(), "eth0");
    // The saved state manages alt-names explicitly: the original kernel
    // name is not auto-added, and the saved alt-name `primary` is applied.
    let alt_names = for_apply.base_iface().alt_names.as_ref().unwrap();
    assert_eq!(alt_names.len(), 1);
    assert_eq!(alt_names[0].name, "primary");
}

#[test]
fn test_mac_id_kernel_iface_name_same_as_current_no_rename() {
    // `kernel-iface-name` equal to the current kernel name: no rename, no
    // auto alt-name (an interface cannot hold an alt-name equal to its own
    // name).
    let merged = gen_merged(
        r#"---
        interfaces:
          - name: port1
            type: ethernet
            identifier: mac-address
            mac-address: 52:54:00:15:17:63
            kernel-iface-name: enp1s0
        "#,
        r#"---
        interfaces:
          - name: enp1s0
            type: ethernet
            mac-address: 52:54:00:15:17:63
        "#,
    )
    .unwrap();
    let merged_iface = merged.ifaces.kernel_ifaces.get("enp1s0").unwrap();
    let for_apply = merged_iface.for_apply.as_ref().unwrap();
    assert_eq!(for_apply.kernel_iface_name(), "enp1s0");
    assert!(for_apply.base_iface().alt_names.is_none());
}

#[test]
fn test_mac_id_kernel_iface_name_persisted_in_for_save() {
    // The rename target `kernel-iface-name` must be persisted in the saved
    // state so the rename is re-applied at boot.
    let merged = gen_merged(
        r#"---
        interfaces:
          - name: port1
            type: ethernet
            identifier: mac-address
            mac-address: 52:54:00:15:17:63
            kernel-iface-name: eth0
        "#,
        r#"---
        interfaces:
          - name: enp1s0
            type: ethernet
            mac-address: 52:54:00:15:17:63
        "#,
    )
    .unwrap();
    let merged_iface = merged.ifaces.kernel_ifaces.get("eth0").unwrap();
    let for_save = merged_iface.for_save.as_ref().unwrap();
    assert_eq!(for_save.kernel_iface_name(), "eth0");
}

#[test]
fn test_mac_id_without_kernel_iface_name_not_persisted() {
    // A MAC-identified config without an explicit `kernel-iface-name` keeps
    // the old behavior: the kernel name is not persisted (the config stays
    // resolvable by MAC at boot).
    let merged = gen_merged(
        r#"---
        interfaces:
          - name: port1
            type: ethernet
            identifier: mac-address
            mac-address: 52:54:00:15:17:63
        "#,
        r#"---
        interfaces:
          - name: enp1s0
            type: ethernet
            mac-address: 52:54:00:15:17:63
        "#,
    )
    .unwrap();
    let merged_iface = merged.ifaces.kernel_ifaces.get("enp1s0").unwrap();
    let for_save = merged_iface.for_save.as_ref().unwrap();
    assert!(for_save.kernel_iface_name().is_empty());
}
