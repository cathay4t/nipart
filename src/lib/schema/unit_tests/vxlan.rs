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

fn sanitize_vxlan(
    iface: &VxlanInterface,
    current: Option<&VxlanInterface>,
) -> Result<(), crate::NipartError> {
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
    )
}

#[test]
fn test_sanitize_vxlan_id_too_large() {
    let iface = vxlan_iface_with_config(VxlanConfig {
        id: Some(16777216),
        base_iface: Some("eth0".to_string()),
        ..Default::default()
    });
    let result = sanitize_vxlan(&iface, None);
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
    let iface = vxlan_iface_with_config(VxlanConfig {
        id: Some(16777215),
        base_iface: Some("eth0".to_string()),
        remote: Some("192.0.2.1".to_string()),
        ..Default::default()
    });
    assert!(sanitize_vxlan(&iface, None).is_ok());
}

#[test]
fn test_sanitize_vxlan_invalid_remote() {
    let iface = vxlan_iface_with_config(VxlanConfig {
        id: Some(100),
        base_iface: Some("eth0".to_string()),
        remote: Some("not-an-ip".to_string()),
        ..Default::default()
    });
    let result = sanitize_vxlan(&iface, None);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
        assert!(
            e.msg().contains("is not valid IP address"),
            "Unexpected error: {}",
            e.msg()
        );
    }
}

#[test]
fn test_sanitize_vxlan_invalid_local() {
    let iface = vxlan_iface_with_config(VxlanConfig {
        id: Some(100),
        base_iface: Some("eth0".to_string()),
        local: Some("bad.local.value".to_string()),
        ..Default::default()
    });
    let result = sanitize_vxlan(&iface, None);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
        assert!(
            e.msg().contains("is not valid IP address"),
            "Unexpected error: {}",
            e.msg()
        );
    }
}

#[test]
fn test_sanitize_vxlan_valid_ips() {
    let iface = vxlan_iface_with_config(VxlanConfig {
        id: Some(100),
        base_iface: Some("eth0".to_string()),
        remote: Some("192.0.2.1".to_string()),
        local: Some("192.0.2.2".to_string()),
        ..Default::default()
    });
    assert!(sanitize_vxlan(&iface, None).is_ok());
}

#[test]
fn test_sanitize_vxlan_missing_config() {
    let iface = VxlanInterface {
        base: BaseInterface {
            name: "vxlan0".to_string(),
            iface_type: InterfaceType::Vxlan,
            state: InterfaceState::Up,
            ..Default::default()
        },
        vxlan: None,
    };
    let result = sanitize_vxlan(&iface, None);
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
        assert!(
            e.msg().contains("vxlan` configuration is mandatory"),
            "Unexpected error: {}",
            e.msg()
        );
    }
}

#[test]
fn test_sanitize_vxlan_missing_config_with_current() {
    let cur = VxlanInterface {
        base: BaseInterface {
            name: "vxlan0".to_string(),
            iface_type: InterfaceType::Vxlan,
            ..Default::default()
        },
        vxlan: Some(VxlanConfig::default()),
    };
    let iface = VxlanInterface {
        base: BaseInterface {
            name: "vxlan0".to_string(),
            iface_type: InterfaceType::Vxlan,
            state: InterfaceState::Absent,
            ..Default::default()
        },
        vxlan: None,
    };
    assert!(sanitize_vxlan(&iface, Some(&cur)).is_ok());
}

#[test]
fn test_sanitize_vxlan_absent_no_config() {
    let iface = VxlanInterface {
        base: BaseInterface {
            name: "vxlan0".to_string(),
            iface_type: InterfaceType::Vxlan,
            state: InterfaceState::Absent,
            ..Default::default()
        },
        vxlan: None,
    };
    assert!(sanitize_vxlan(&iface, None).is_ok());
}
