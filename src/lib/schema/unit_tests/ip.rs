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

