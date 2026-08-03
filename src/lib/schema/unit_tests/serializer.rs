// SPDX-License-Identifier: Apache-2.0

use serde::Serialize;

use crate::serializer::{is_option_string_empty, option_u32_as_hex};

#[derive(Serialize)]
struct TestHexU32 {
    #[serde(serialize_with = "option_u32_as_hex")]
    v: Option<u32>,
}

#[test]
fn test_serialize_is_option_string_empty_none() {
    assert!(is_option_string_empty(&None));
}

#[test]
fn test_serialize_is_option_string_empty_empty_str() {
    assert!(is_option_string_empty(&Some(String::new())));
}

