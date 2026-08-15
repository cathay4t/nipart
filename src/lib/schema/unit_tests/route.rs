// SPDX-License-Identifier: Apache-2.0

use crate::{
    Interfaces, MergedInterfaces, MergedRoutes, RouteEntry, RouteState, Routes,
};

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
