// SPDX-License-Identifier: Apache-2.0

use super::super::ip::sanitize_ip_network;
use crate::ErrorKind;

#[test]
fn test_sanitize_ip_network_empty_str() {
    let result = sanitize_ip_network("");
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
    }
}

#[test]
fn test_sanitize_ip_network_invalid_str() {
    let result = sanitize_ip_network("192.0.2.1/24/");
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
    }
}

#[test]
fn test_sanitize_ip_network_invalid_ipv4_prefix_length() {
    let result = sanitize_ip_network("192.0.2.1/33");
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
    }
}

