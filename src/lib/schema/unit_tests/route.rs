// SPDX-License-Identifier: Apache-2.0

use crate::{
    Interfaces, MergedInterfaces, MergedRoutes, RouteEntry, RouteState, Routes,
};

#[test]
fn test_desired_route_matching_running_kernel_route_not_changed() {
    let des_ifaces: Interfaces = rmsd_yaml::from_str(
        r#"
    - name: dummy0
      type: dummy
      state: up
      ipv4:
        enabled: true
        dhcp: false
        address:
        - ip: 198.51.100.10
          prefix-length: 24
    "#,
    )
    .unwrap();
    let merged_ifaces =
        MergedInterfaces::new(des_ifaces, Interfaces::default(), None).unwrap();

    let des_routes: Routes = rmsd_yaml::from_str(
        r#"---
        config:
          - destination: 198.51.100.0/24
            next-hop-interface: dummy0
        "#,
    )
    .unwrap();
    // The materialized kernel route is only reported under `running` (e.g.
    // a `proto kernel` connected route created by the address assignment).
    let cur_routes: Routes = rmsd_yaml::from_str(
        r#"---
        running:
          - destination: 198.51.100.0/24
            next-hop-interface: dummy0
            next-hop-address: 0.0.0.0
        "#,
    )
    .unwrap();

    let merged_routes =
        MergedRoutes::new(des_routes, cur_routes, None, &merged_ifaces)
            .unwrap();

    assert!(merged_routes.changed_routes.is_empty());
}

#[test]
fn test_saved_route_not_applied_but_persisted() {
    let des_ifaces: Interfaces = rmsd_yaml::from_str(
        r#"
    - name: dummy0
      type: dummy
      state: up
    "#,
    )
    .unwrap();
    let merged_ifaces =
        MergedInterfaces::new(des_ifaces, Interfaces::default(), None).unwrap();

    let des_routes: Routes = rmsd_yaml::from_str(
        r#"---
        config:
          - destination: 198.51.100.0/24
            next-hop-interface: dummy0
            state: saved
        "#,
    )
    .unwrap();

    let merged_routes =
        MergedRoutes::new(des_routes, Routes::default(), None, &merged_ifaces)
            .unwrap();

    assert!(merged_routes.changed_routes.is_empty());
    assert!(merged_routes.merged.is_empty());
    assert!(merged_routes.route_changed_ifaces.is_empty());

    let saved_rts = merged_routes.gen_state_for_save().config.unwrap();
    assert_eq!(saved_rts.len(), 1);
    assert!(saved_rts[0].is_saved());
}

#[test]
fn test_route_of_saved_only_iface_not_applied_but_persisted() {
    let des_ifaces: Interfaces = rmsd_yaml::from_str(
        r#"
    - name: cunet
      type: ethernet
      state: saved
    "#,
    )
    .unwrap();
    let merged_ifaces =
        MergedInterfaces::new(des_ifaces, Interfaces::default(), None).unwrap();

    let des_routes: Routes = rmsd_yaml::from_str(
        r#"---
        config:
          - destination: 198.51.100.0/24
            next-hop-interface: cunet
        "#,
    )
    .unwrap();

    let merged_routes =
        MergedRoutes::new(des_routes, Routes::default(), None, &merged_ifaces)
            .unwrap();

    // The route is not applied while its profile is save-only, but it is
    // persisted so `npt up` can activate it later.
    assert!(merged_routes.changed_routes.is_empty());
    let saved_rts = merged_routes.gen_state_for_save().config.unwrap();
    assert_eq!(saved_rts.len(), 1);
    assert!(!saved_rts[0].is_saved());
}

#[test]
fn test_absent_iface_does_not_mark_routes_absent() {
    let des_ifaces = r#"
    - name: cn
      type: dummy
      state: absent
    "#;
    let cur_ifaces = r#"
    - name: cn
      type: dummy
      state: up
    "#;
    let des_ifaces: Interfaces = rmsd_yaml::from_str(des_ifaces).unwrap();
    let cur_ifaces: Interfaces = rmsd_yaml::from_str(cur_ifaces).unwrap();
    let merged_ifaces =
        MergedInterfaces::new(des_ifaces, cur_ifaces, None).unwrap();

    let des_routes: Routes = rmsd_yaml::from_str(
        r#"---
        config:
          - destination: 198.51.100.0/24
            next-hop-interface: cn
            state: absent
        "#,
    )
    .unwrap();
    let cur_routes: Routes = rmsd_yaml::from_str(
        r#"---
        config:
          - destination: 198.51.100.0/24
            next-hop-interface: cn
        "#,
    )
    .unwrap();

    let merged_routes =
        MergedRoutes::new(des_routes, cur_routes, None, &merged_ifaces)
            .unwrap();

    // The interface is deleted by the kernel, which purges its routes, so
    // there is no need to remove them explicitly.
    assert!(merged_routes.changed_routes.is_empty());
    assert!(merged_routes.merged.is_empty());
}

#[test]
fn test_down_iface_still_marks_routes_absent() {
    let des_ifaces = r#"
    - name: eth1
      type: ethernet
      state: down
      ipv4:
        enabled: false
      ipv6:
        enabled: false
    "#;
    let cur_ifaces = r#"
    - name: eth1
      type: ethernet
      state: up
    "#;
    let des_ifaces: Interfaces = rmsd_yaml::from_str(des_ifaces).unwrap();
    let cur_ifaces: Interfaces = rmsd_yaml::from_str(cur_ifaces).unwrap();
    let merged_ifaces =
        MergedInterfaces::new(des_ifaces, cur_ifaces, None).unwrap();

    let des_routes: Routes = rmsd_yaml::from_str(
        r#"---
        config:
          - destination: 198.51.100.0/24
            next-hop-interface: eth1
            state: absent
        "#,
    )
    .unwrap();
    let cur_routes: Routes = rmsd_yaml::from_str(
        r#"---
        config:
          - destination: 198.51.100.0/24
            next-hop-interface: eth1
        "#,
    )
    .unwrap();

    let merged_routes =
        MergedRoutes::new(des_routes, cur_routes, None, &merged_ifaces)
            .unwrap();

    // A down (not deleted) interface keeps its link, so its routes must be
    // removed explicitly.
    let absent_routes: Vec<&RouteEntry> = merged_routes
        .changed_routes
        .iter()
        .filter(|rt| rt.state == Some(RouteState::Absent))
        .collect();
    assert_eq!(absent_routes.len(), 1);
    assert_eq!(absent_routes[0].next_hop_iface.as_deref(), Some("eth1"));
}
