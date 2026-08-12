// SPDX-License-Identifier: Apache-2.0

use crate::{
    BaseInterface, ErrorKind, Interface, InterfaceState, InterfaceType,
    Interfaces, MergedInterfaces, MergedRoutes, NipartInterface, RouteEntry,
    Routes, VrfConfig, VrfInterface,
};

fn vrf_iface_with_config(vrf: VrfConfig) -> VrfInterface {
    VrfInterface {
        base: BaseInterface {
            name: "vrf0".to_string(),
            iface_type: InterfaceType::Vrf,
            state: InterfaceState::Up,
            ..Default::default()
        },
        vrf: Some(vrf),
    }
}

fn sanitize_vrf(
    iface: &VrfInterface,
    current: Option<&VrfInterface>,
) -> Result<(VrfInterface, VrfInterface), crate::NipartError> {
    let mut for_save = iface.clone();
    let mut for_apply = iface.clone();
    let mut for_verify = iface.clone();
    let mut merged = iface.clone();
    iface.sanitize(
        current,
        &mut for_save,
        &mut for_apply,
        &mut for_verify,
        &mut merged,
    )?;
    Ok((for_apply, for_verify))
}

fn sanitize_vrf_for_save(
    iface: &VrfInterface,
    current: Option<&VrfInterface>,
) -> Result<VrfInterface, crate::NipartError> {
    let mut for_save = iface.clone();
    let mut for_apply = iface.clone();
    let mut for_verify = iface.clone();
    let mut merged = iface.clone();
    iface.sanitize(
        current,
        &mut for_save,
        &mut for_apply,
        &mut for_verify,
        &mut merged,
    )?;
    Ok(for_save)
}

#[test]
fn test_vrf_de_stringlized_table_id() {
    let iface: VrfInterface = rmsd_yaml::from_str(
        r#"---
name: vrf1
type: vrf
state: up
vrf:
  route-table-id: "101"
"#,
    )
    .unwrap();

    assert_eq!(iface.vrf.unwrap().table_id, Some(101));
}

#[test]
fn test_vrf_de_ports() {
    let iface: VrfInterface = rmsd_yaml::from_str(
        r#"---
name: vrf1
type: vrf
state: up
vrf:
  route-table-id: "101"
  ports:
    - eth1
    - eth2
"#,
    )
    .unwrap();

    assert_eq!(
        iface.vrf.as_ref().unwrap().ports,
        Some(vec!["eth1".to_string(), "eth2".to_string()])
    );
}

#[test]
fn test_vrf_de_legacy_port_alias() {
    let iface: VrfInterface = rmsd_yaml::from_str(
        r#"---
name: vrf1
type: vrf
state: up
vrf:
  route-table-id: "101"
  port:
    - eth1
    - eth2
"#,
    )
    .unwrap();

    assert_eq!(
        iface.vrf.as_ref().unwrap().ports,
        Some(vec!["eth1".to_string(), "eth2".to_string()])
    );
}

#[test]
fn test_vrf_skip_port_if_null() {
    let iface = vrf_iface_with_config(VrfConfig {
        table_id: Some(101),
        ..Default::default()
    });

    let iface_yaml = rmsd_yaml::to_string(&iface).unwrap();

    assert!(!iface_yaml.contains("ports:"));
    assert!(iface_yaml.contains("route-table-id: 101"));
}

#[test]
fn test_vrf_serialize_ports_native_name() {
    let iface = vrf_iface_with_config(VrfConfig {
        table_id: Some(100),
        ports: Some(vec!["eth1".to_string()]),
    });

    let iface_yaml = rmsd_yaml::to_string(&iface).unwrap();

    assert!(iface_yaml.contains("ports:"));
}

#[test]
fn test_vrf_sanitize_ignore_mac_address() {
    let iface = VrfInterface {
        base: BaseInterface {
            name: "vrf0".to_string(),
            iface_type: InterfaceType::Vrf,
            state: InterfaceState::Up,
            mac_address: Some("DE:AD:BE:EF:00:01".to_string()),
            ..Default::default()
        },
        vrf: Some(VrfConfig {
            table_id: Some(100),
            ports: Some(vec!["eth2".to_string(), "eth1".to_string()]),
        }),
    };

    let (for_apply, for_verify) = sanitize_vrf(&iface, None).unwrap();

    assert_eq!(for_apply.base.mac_address, None);
    assert_eq!(for_verify.base.mac_address, None);
    // Ports should be sorted.
    assert_eq!(
        for_apply.vrf.unwrap().ports,
        Some(vec!["eth1".to_string(), "eth2".to_string()])
    );
}

#[test]
fn test_vrf_sanitize_new_vrf_without_table_id() {
    let iface = vrf_iface_with_config(VrfConfig::default());

    let result = sanitize_vrf(&iface, None);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
        assert!(
            e.msg().contains("route-table-id"),
            "Unexpected error: {}",
            e.msg()
        );
    }
}

#[test]
fn test_vrf_sanitize_new_vrf_table_id_zero() {
    let iface = vrf_iface_with_config(VrfConfig {
        table_id: Some(0),
        ..Default::default()
    });

    let result = sanitize_vrf(&iface, None);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
    }
}

#[test]
fn test_vrf_sanitize_new_vrf_without_vrf_section() {
    let iface = VrfInterface {
        base: BaseInterface {
            name: "vrf0".to_string(),
            iface_type: InterfaceType::Vrf,
            state: InterfaceState::Up,
            ..Default::default()
        },
        vrf: None,
    };

    let result = sanitize_vrf(&iface, None);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
    }
}

#[test]
fn test_vrf_sanitize_fill_table_id_from_current() {
    let desired = vrf_iface_with_config(VrfConfig::default());
    let current = vrf_iface_with_config(VrfConfig {
        table_id: Some(100),
        ports: Some(vec!["eth1".to_string()]),
    });

    let (for_apply, for_verify) =
        sanitize_vrf(&desired, Some(&current)).unwrap();

    assert_eq!(for_apply.vrf.unwrap().table_id, Some(100));
    assert_eq!(for_verify.vrf.unwrap().table_id, Some(100));
}

#[test]
fn test_vrf_sanitize_for_save_keeps_current_ports() {
    // The saved config must stay complete (ports + table ID) so the daemon
    // can restore the whole VRF from it after restart, even when the desired
    // state only touches the table ID.
    let desired = vrf_iface_with_config(VrfConfig {
        table_id: Some(101),
        ..Default::default()
    });
    let current = vrf_iface_with_config(VrfConfig {
        table_id: Some(100),
        ports: Some(vec!["eth2".to_string(), "eth1".to_string()]),
    });

    let for_save = sanitize_vrf_for_save(&desired, Some(&current)).unwrap();

    let for_save_conf = for_save.vrf.unwrap();
    assert_eq!(for_save_conf.table_id, Some(101));
    assert_eq!(
        for_save_conf.ports,
        Some(vec!["eth1".to_string(), "eth2".to_string()])
    );
}

#[test]
fn test_vrf_sanitize_for_save_keeps_current_vrf_section() {
    // When the desired state does not define the `vrf` section at all (e.g.
    // only MTU is changed), the saved config must still carry the whole VRF
    // section so boot restore can recreate the interface.
    let desired = VrfInterface {
        base: BaseInterface {
            name: "vrf0".to_string(),
            iface_type: InterfaceType::Vrf,
            state: InterfaceState::Up,
            mtu: Some(1400),
            ..Default::default()
        },
        vrf: None,
    };
    let current = vrf_iface_with_config(VrfConfig {
        table_id: Some(100),
        ports: Some(vec!["eth1".to_string()]),
    });

    let for_save = sanitize_vrf_for_save(&desired, Some(&current)).unwrap();

    let for_save_conf = for_save.vrf.unwrap();
    assert_eq!(for_save_conf.table_id, Some(100));
    assert_eq!(for_save_conf.ports, Some(vec!["eth1".to_string()]));
}

#[test]
fn test_vrf_sanitize_for_save_keeps_empty_ports() {
    // An explicit empty port list means "remove all ports" and must NOT be
    // overridden with the current ports in the saved config.
    let desired = vrf_iface_with_config(VrfConfig {
        table_id: Some(100),
        ports: Some(Vec::new()),
    });
    let current = vrf_iface_with_config(VrfConfig {
        table_id: Some(100),
        ports: Some(vec!["eth1".to_string()]),
    });

    let for_save = sanitize_vrf_for_save(&desired, Some(&current)).unwrap();

    assert_eq!(for_save.vrf.unwrap().ports, Some(Vec::new()));
}

#[test]
fn test_vrf_sanitize_absent_no_config() {
    let iface = VrfInterface {
        base: BaseInterface {
            name: "vrf0".to_string(),
            iface_type: InterfaceType::Vrf,
            state: InterfaceState::Absent,
            ..Default::default()
        },
        vrf: None,
    };

    let result = sanitize_vrf(&iface, None);
    assert!(result.is_ok());
}

fn merged_ifaces_with_vrf(
    desired: &str,
    current: &str,
) -> Result<MergedInterfaces, crate::NipartError> {
    let des_ifaces: Interfaces = rmsd_yaml::from_str(desired).unwrap();
    let cur_ifaces: Interfaces = rmsd_yaml::from_str(current).unwrap();
    MergedInterfaces::new(des_ifaces, cur_ifaces, None)
}

#[test]
fn test_route_vrf_name_resolve() {
    let cur_ifaces = r#"
    - name: vrf0
      type: vrf
      state: up
      vrf:
        route-table-id: 100
    "#;
    let des_ifaces = "[]";

    let merged_ifaces = merged_ifaces_with_vrf(des_ifaces, cur_ifaces).unwrap();

    let des_routes: Routes = rmsd_yaml::from_str(
        r#"---
        config:
          - destination: 198.51.200.0/24
            route-type: blackhole
            vrf-name: vrf0
        "#,
    )
    .unwrap();

    let merged_routes =
        MergedRoutes::new(des_routes, Routes::default(), None, &merged_ifaces)
            .unwrap();

    assert_eq!(
        merged_routes.desired.config.as_ref().unwrap()[0].table_id,
        Some(100)
    );
}

#[test]
fn test_route_vrf_name_down() {
    let cur_ifaces = r#"
    - name: vrf0
      type: vrf
      state: up
      vrf:
        route-table-id: 100
    "#;
    let des_ifaces = r#"
    - name: vrf0
      type: vrf
      state: down
    "#;

    let merged_ifaces = merged_ifaces_with_vrf(des_ifaces, cur_ifaces).unwrap();

    let des_routes: Routes = rmsd_yaml::from_str(
        r#"---
        config:
          - destination: 198.51.200.0/24
            route-type: blackhole
            vrf-name: vrf0
        "#,
    )
    .unwrap();

    let result =
        MergedRoutes::new(des_routes, Routes::default(), None, &merged_ifaces);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
    }
}

#[test]
fn test_route_vrf_name_absent() {
    let cur_ifaces = r#"
    - name: vrf0
      type: vrf
      state: up
      vrf:
        route-table-id: 100
    "#;
    let des_ifaces = r#"
    - name: vrf0
      type: vrf
      state: absent
    "#;

    let merged_ifaces = merged_ifaces_with_vrf(des_ifaces, cur_ifaces).unwrap();

    let des_routes: Routes = rmsd_yaml::from_str(
        r#"---
        config:
          - destination: 198.51.200.0/24
            route-type: blackhole
            vrf-name: vrf0
        "#,
    )
    .unwrap();

    let result =
        MergedRoutes::new(des_routes, Routes::default(), None, &merged_ifaces);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
    }
}

#[test]
fn test_route_vrf_name_table_id_conflict() {
    let cur_ifaces = r#"
    - name: vrf0
      type: vrf
      state: up
      vrf:
        route-table-id: 100
    "#;

    let merged_ifaces = merged_ifaces_with_vrf("[]", cur_ifaces).unwrap();

    let des_routes: Routes = rmsd_yaml::from_str(
        r#"---
        config:
          - destination: 198.51.200.0/24
            route-type: blackhole
            table-id: 101
            vrf-name: vrf0
        "#,
    )
    .unwrap();

    let result =
        MergedRoutes::new(des_routes, Routes::default(), None, &merged_ifaces);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
    }
}

#[test]
fn test_route_vrf_name_not_exist() {
    let merged_ifaces = merged_ifaces_with_vrf("[]", "[]").unwrap();

    let des_routes: Routes = rmsd_yaml::from_str(
        r#"---
        config:
          - destination: 198.51.200.0/24
            route-type: blackhole
            vrf-name: vrf1
        "#,
    )
    .unwrap();

    let result =
        MergedRoutes::new(des_routes, Routes::default(), None, &merged_ifaces);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
    }
}

#[test]
fn test_vrf_roundtrip() {
    let iface = vrf_iface_with_config(VrfConfig {
        table_id: Some(100),
        ports: Some(vec!["eth1".to_string(), "eth2".to_string()]),
    });

    let yaml_str = rmsd_yaml::to_string(&iface).unwrap();
    let reparsed: VrfInterface = rmsd_yaml::from_str(&yaml_str).unwrap();
    assert_eq!(iface, reparsed);
}

#[test]
fn test_vrf_merge_with_current_keeps_ports() {
    let des_ifaces: Interfaces = rmsd_yaml::from_str(
        r#"---
    - name: vrf0
      type: vrf
      state: up
      vrf:
        route-table-id: 101
    "#,
    )
    .unwrap();
    let cur_ifaces: Interfaces = rmsd_yaml::from_str(
        r#"---
    - name: vrf0
      type: vrf
      state: up
      vrf:
        route-table-id: 100
        ports:
        - eth1
        - eth2
    - name: eth1
      type: ethernet
      state: up
      controller: vrf0
    - name: eth2
      type: ethernet
      state: up
      controller: vrf0
    "#,
    )
    .unwrap();

    let merged = MergedInterfaces::new(des_ifaces, cur_ifaces, None).unwrap();
    let merged_vrf = merged.kernel_ifaces.get("vrf0").unwrap();
    assert_eq!(merged_vrf.merged.ports(), Some(vec!["eth1", "eth2"]),);
    assert_eq!(merged_vrf.for_apply.as_ref().unwrap().name(), "vrf0",);
}

#[test]
fn test_vrf_controller_port_relationship() {
    let des_ifaces: Interfaces = rmsd_yaml::from_str(
        r#"---
    - name: vrf0
      type: vrf
      state: up
      vrf:
        route-table-id: 100
        ports:
        - eth1
    "#,
    )
    .unwrap();
    let cur_ifaces: Interfaces = rmsd_yaml::from_str(
        r#"---
    - name: eth1
      type: ethernet
      state: up
    "#,
    )
    .unwrap();

    let merged = MergedInterfaces::new(des_ifaces, cur_ifaces, None).unwrap();
    let eth1 = merged.kernel_ifaces.get("eth1").unwrap();
    // Port should get controller assigned automatically.
    assert_eq!(
        eth1.for_apply
            .as_ref()
            .unwrap()
            .base_iface()
            .controller
            .as_deref(),
        Some("vrf0"),
    );
    let vrf0 = merged.kernel_ifaces.get("vrf0").unwrap();
    // Controller should be applied before its ports.
    assert!(
        vrf0.up_priority.unwrap_or_default()
            < eth1.up_priority.unwrap_or_default()
    );
}

#[test]
fn test_vrf_deserialize_via_interface_enum() {
    let iface: Interface = rmsd_yaml::from_str(
        r#"---
name: vrf0
type: vrf
state: up
vrf:
  route-table-id: 100
"#,
    )
    .unwrap();
    assert!(matches!(iface, Interface::Vrf(_)));
}

#[test]
fn test_vrf_deserialize_absent_keeps_only_type() {
    let iface: Interface = rmsd_yaml::from_str(
        r#"---
name: vrf0
type: vrf
state: absent
vrf:
  route-table-id: 100
"#,
    )
    .unwrap();
    match &iface {
        Interface::Vrf(v) => {
            assert_eq!(v.vrf, None);
        }
        _ => panic!("Expected VRF interface"),
    }
}

#[test]
fn test_route_vrf_name_keeps_vrf_name_in_save() {
    let rt: RouteEntry = rmsd_yaml::from_str(
        r#"---
        destination: 198.51.200.0/24
        route-type: blackhole
        vrf-name: vrf0
        "#,
    )
    .unwrap();
    assert_eq!(rt.vrf_name.as_deref(), Some("vrf0"));
    let yaml_str = rmsd_yaml::to_string(&rt).unwrap();
    assert!(yaml_str.contains("vrf-name: vrf0"));
}

#[test]
fn test_vrf_resolve_port_ref_by_mac_identifier() {
    let desired: Interfaces = rmsd_yaml::from_str(
        r#"---
        - name: vrf0
          type: vrf
          state: up
          vrf:
            route-table-id: 100
            ports:
            - port1
            - port2
        - name: port1
          type: ethernet
          mac-address: 00:23:45:67:89:1a
          identifier: mac-address
        - name: port2
          type: ethernet
          mac-address: 00:23:45:67:89:1b
          identifier: mac-address"#,
    )
    .unwrap();

    let current: Interfaces = rmsd_yaml::from_str(
        r#"---
        - name: eth0
          type: ethernet
          mac-address: 00:23:45:67:89:1a
        - name: eth1
          type: ethernet
          mac-address: 00:23:45:67:89:1b"#,
    )
    .unwrap();

    let merged = MergedInterfaces::new(desired, current, None).unwrap();

    let vrf = merged.kernel_ifaces.get("vrf0").unwrap();

    // Port profile names are resolved to kernel interface names for
    // apply/verify/merged.
    assert_eq!(
        vrf.for_apply.as_ref().unwrap().ports().unwrap(),
        vec!["eth0", "eth1"]
    );
    assert_eq!(
        vrf.for_verify.as_ref().unwrap().ports().unwrap(),
        vec!["eth0", "eth1"]
    );
    assert_eq!(vrf.merged.ports().unwrap(), vec!["eth0", "eth1"]);
    // The saved config keeps the profile names so the VRF can be restored
    // by MAC identifier after daemon restart.
    assert_eq!(
        vrf.for_save.as_ref().unwrap().ports().unwrap(),
        vec!["port1", "port2"]
    );
}
