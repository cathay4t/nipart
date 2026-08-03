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

#[test]
fn test_serialize_is_option_string_empty_non_empty() {
    assert!(!is_option_string_empty(&Some("hello".to_string())));
}

#[test]
fn test_serialize_option_u32_as_hex_some() {
    let t = TestHexU32 { v: Some(255) };
    let value: serde_yaml::Value = serde_yaml::to_value(&t).unwrap();
    assert_eq!(
        value.get("v"),
        Some(&serde_yaml::Value::String("0xff".to_string()))
    );
}

#[test]
fn test_serialize_option_u32_as_hex_none() {
    let t = TestHexU32 { v: None };
    let value: serde_yaml::Value = serde_yaml::to_value(&t).unwrap();
    assert_eq!(value.get("v"), Some(&serde_yaml::Value::Null));
}
