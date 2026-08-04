// SPDX-License-Identifier: Apache-2.0

use crate::{
    BaseInterface, ErrorKind, InterfaceState, InterfaceType, NipartInterface,
    VxlanConfig, VxlanInterface,
};

fn vxlan_iface_with_config(vxlan: VxlanConfig) -> VxlanInterface {
    VxlanInterface {
        base: BaseInterface {
            name: "vxlan0".to_string(),
            iface_type: InterfaceType::Vxlan,
            state: InterfaceState::Up,
            ..Default::default()
        },
        vxlan: Some(vxlan),
    }
}

#[test]
fn test_sanitize_vxlan_id_too_large() {
    let mut iface = vxlan_iface_with_config(VxlanConfig {
        id: Some(16777216),
        base_iface: Some("eth0".to_string()),
        ..Default::default()
    });
    let result = iface.sanitize(None);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
        assert!(
            e.msg().contains("VNI must be 0-16777215"),
            "Unexpected error: {}",
            e.msg()
        );
    }
}

#[test]
fn test_sanitize_vxlan_id_max_valid() {
    let mut iface = vxlan_iface_with_config(VxlanConfig {
        id: Some(16777215),
        base_iface: Some("eth0".to_string()),
        ..Default::default()
    });
    assert!(iface.sanitize(None).is_ok());
}

#[test]
fn test_sanitize_vxlan_invalid_remote() {
    let mut iface = vxlan_iface_with_config(VxlanConfig {
        id: Some(100),
        base_iface: Some("eth0".to_string()),
        remote: Some("not-an-ip".to_string()),
        ..Default::default()
    });
    let result = iface.sanitize(None);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
        assert!(
            e.msg().contains("not a valid IP address"),
            "Unexpected error: {}",
            e.msg()
        );
    }
}

#[test]
fn test_sanitize_vxlan_invalid_local() {
    let mut iface = vxlan_iface_with_config(VxlanConfig {
        id: Some(100),
        base_iface: Some("eth0".to_string()),
        local: Some("bad.local.value".to_string()),
        ..Default::default()
    });
    let result = iface.sanitize(None);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
        assert!(
            e.msg().contains("not a valid IP address"),
            "Unexpected error: {}",
            e.msg()
        );
    }
}

#[test]
fn test_sanitize_vxlan_valid_ips() {
    let mut iface = vxlan_iface_with_config(VxlanConfig {
        id: Some(100),
        base_iface: Some("eth0".to_string()),
        remote: Some("192.0.2.1".to_string()),
        local: Some("192.0.2.2".to_string()),
        ..Default::default()
    });
    assert!(iface.sanitize(None).is_ok());
}

