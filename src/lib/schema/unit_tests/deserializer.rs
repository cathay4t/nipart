// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;

use crate::BondMode;
use crate::deserializer::{
    bool_or_string, number_as_string, option_bool_or_string,
    option_enum_string_or_integer, option_i32_or_string, option_i64_or_string,
    option_number_as_string, option_u8_or_string, option_u16_or_string,
    option_u32_or_string, option_u64_or_string, u8_or_string, u16_or_string,
    u32_or_string,
};

#[derive(Debug, PartialEq, Deserialize)]
struct TestOptionBool {
    #[serde(default, deserialize_with = "option_bool_or_string")]
    v: Option<bool>,
}

#[derive(Debug, PartialEq, Deserialize)]
struct TestRequiredBool {
    #[serde(deserialize_with = "bool_or_string")]
    v: bool,
}

#[derive(Debug, PartialEq, Deserialize)]
struct TestOptionU8 {
    #[serde(default, deserialize_with = "option_u8_or_string")]
    v: Option<u8>,
}

#[derive(Debug, PartialEq, Deserialize)]
struct TestRequiredU8 {
    #[serde(deserialize_with = "u8_or_string")]
    v: u8,
}

#[derive(Debug, PartialEq, Deserialize)]
struct TestOptionU16 {
    #[serde(default, deserialize_with = "option_u16_or_string")]
    v: Option<u16>,
}

#[derive(Debug, PartialEq, Deserialize)]
struct TestRequiredU16 {
    #[serde(deserialize_with = "u16_or_string")]
    v: u16,
}

#[derive(Debug, PartialEq, Deserialize)]
struct TestOptionU32 {
    #[serde(default, deserialize_with = "option_u32_or_string")]
    v: Option<u32>,
}

#[derive(Debug, PartialEq, Deserialize)]
struct TestRequiredU32 {
    #[serde(deserialize_with = "u32_or_string")]
    v: u32,
}

#[derive(Debug, PartialEq, Deserialize)]
struct TestOptionU64 {
    #[serde(default, deserialize_with = "option_u64_or_string")]
    v: Option<u64>,
}

#[derive(Debug, PartialEq, Deserialize)]
struct TestOptionI32 {
    #[serde(default, deserialize_with = "option_i32_or_string")]
    v: Option<i32>,
}

#[derive(Debug, PartialEq, Deserialize)]
struct TestOptionI64 {
    #[serde(default, deserialize_with = "option_i64_or_string")]
    v: Option<i64>,
}

#[derive(Debug, PartialEq, Deserialize)]
struct TestOptionNumberAsString {
    #[serde(default, deserialize_with = "option_number_as_string")]
    v: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize)]
struct TestRequiredNumberAsString {
    #[serde(deserialize_with = "number_as_string")]
    v: String,
}

#[derive(Debug, PartialEq, Deserialize)]
struct TestOptionBondMode {
    #[serde(default, deserialize_with = "option_enum_string_or_integer")]
    v: Option<BondMode>,
}

#[test]
fn test_de_option_bool_native() {
    let t: TestOptionBool = serde_yaml::from_str("v: true").unwrap();
    assert_eq!(t.v, Some(true));

    let t: TestOptionBool = serde_yaml::from_str("v: false").unwrap();
    assert_eq!(t.v, Some(false));
}

#[test]
fn test_de_option_bool_string_truthy() {
    for s in ["1", "true", "yes", "on", "y"] {
        let t: TestOptionBool =
            serde_yaml::from_str(&format!("v: \"{s}\"")).unwrap();
        assert_eq!(t.v, Some(true), "string {s}");
    }
}

#[test]
fn test_de_option_bool_string_falsy() {
    for s in ["0", "false", "no", "off", "n"] {
        let t: TestOptionBool =
            serde_yaml::from_str(&format!("v: \"{s}\"")).unwrap();
        assert_eq!(t.v, Some(false), "string {s}");
    }
}

#[test]
fn test_de_option_bool_string_case_insensitive() {
    let t: TestOptionBool = serde_yaml::from_str("v: \"TRUE\"").unwrap();
    assert_eq!(t.v, Some(true));

    let t: TestOptionBool = serde_yaml::from_str("v: \"No\"").unwrap();
    assert_eq!(t.v, Some(false));
}

#[test]
fn test_de_option_bool_integer() {
    let t: TestOptionBool = serde_yaml::from_str("v: 1").unwrap();
    assert_eq!(t.v, Some(true));

    let t: TestOptionBool = serde_yaml::from_str("v: 0").unwrap();
    assert_eq!(t.v, Some(false));
}

#[test]
fn test_de_option_bool_absent_is_none() {
    let t: TestOptionBool = serde_yaml::from_str("{}").unwrap();
    assert_eq!(t.v, None);
}

#[test]
fn test_de_option_bool_invalid_string() {
    let result: Result<TestOptionBool, _> =
        serde_yaml::from_str("v: \"maybe\"");
    assert!(result.is_err());
}

#[test]
fn test_de_option_bool_invalid_integer() {
    let result: Result<TestOptionBool, _> = serde_yaml::from_str("v: 2");
    assert!(result.is_err());
}

#[test]
fn test_de_option_u64_native_integer() {
    let t: TestOptionU64 = serde_yaml::from_str("v: 42").unwrap();
    assert_eq!(t.v, Some(42));
}

#[test]
fn test_de_option_u64_decimal_string() {
    let t: TestOptionU64 = serde_yaml::from_str("v: \"42\"").unwrap();
    assert_eq!(t.v, Some(42));
}

#[test]
fn test_de_option_u64_hex_string() {
    let t: TestOptionU64 = serde_yaml::from_str("v: \"0xff\"").unwrap();
    assert_eq!(t.v, Some(255));
}

#[test]
fn test_de_option_u64_invalid_string() {
    let result: Result<TestOptionU64, _> =
        serde_yaml::from_str("v: \"not-a-number\"");
    assert!(result.is_err());
}

#[test]
fn test_de_option_u64_absent_is_none() {
    let t: TestOptionU64 = serde_yaml::from_str("{}").unwrap();
    assert_eq!(t.v, None);
}

#[test]
fn test_de_option_u8_valid() {
    let t: TestOptionU8 = serde_yaml::from_str("v: 255").unwrap();
    assert_eq!(t.v, Some(255));

    let t: TestOptionU8 = serde_yaml::from_str("v: \"200\"").unwrap();
    assert_eq!(t.v, Some(200));
}

#[test]
fn test_de_option_u8_overflow() {
    let result: Result<TestOptionU8, _> = serde_yaml::from_str("v: 256");
    assert!(result.is_err());
}

#[test]
fn test_de_option_u8_absent_is_none() {
    let t: TestOptionU8 = serde_yaml::from_str("{}").unwrap();
    assert_eq!(t.v, None);
}

#[test]
fn test_de_option_u16_valid() {
    let t: TestOptionU16 = serde_yaml::from_str("v: 65535").unwrap();
    assert_eq!(t.v, Some(65535));

    let t: TestOptionU16 = serde_yaml::from_str("v: \"4096\"").unwrap();
    assert_eq!(t.v, Some(4096));
}

#[test]
fn test_de_option_u16_overflow() {
    let result: Result<TestOptionU16, _> = serde_yaml::from_str("v: 65536");
    assert!(result.is_err());
}

#[test]
fn test_de_option_u16_absent_is_none() {
    let t: TestOptionU16 = serde_yaml::from_str("{}").unwrap();
    assert_eq!(t.v, None);
}

#[test]
fn test_de_option_u32_valid() {
    let t: TestOptionU32 = serde_yaml::from_str("v: 4294967295").unwrap();
    assert_eq!(t.v, Some(u32::MAX));

    let t: TestOptionU32 = serde_yaml::from_str("v: \"4096\"").unwrap();
    assert_eq!(t.v, Some(4096));
}

#[test]
fn test_de_option_u32_overflow() {
    let result: Result<TestOptionU32, _> =
        serde_yaml::from_str("v: 4294967296");
    assert!(result.is_err());
}

#[test]
fn test_de_option_u32_absent_is_none() {
    let t: TestOptionU32 = serde_yaml::from_str("{}").unwrap();
    assert_eq!(t.v, None);
}

#[test]
fn test_de_option_i64_native_integer() {
    let t: TestOptionI64 = serde_yaml::from_str("v: 42").unwrap();
    assert_eq!(t.v, Some(42));

    let t: TestOptionI64 = serde_yaml::from_str("v: -42").unwrap();
    assert_eq!(t.v, Some(-42));
}

#[test]
fn test_de_option_i64_unsigned_overflow() {
    // i64::MAX + 1, parsed as u64 by YAML, cannot fit into i64
    let result: Result<TestOptionI64, _> =
        serde_yaml::from_str("v: 9223372036854775808");
    assert!(result.is_err());
}

#[test]
fn test_de_option_i64_string() {
    let t: TestOptionI64 = serde_yaml::from_str("v: \"-5\"").unwrap();
    assert_eq!(t.v, Some(-5));

    let t: TestOptionI64 = serde_yaml::from_str("v: \"5\"").unwrap();
    assert_eq!(t.v, Some(5));
}

#[test]
fn test_de_option_i64_invalid_string() {
    let result: Result<TestOptionI64, _> =
        serde_yaml::from_str("v: \"not-a-number\"");
    assert!(result.is_err());
}

#[test]
fn test_de_option_i64_absent_is_none() {
    let t: TestOptionI64 = serde_yaml::from_str("{}").unwrap();
    assert_eq!(t.v, None);
}

#[test]
fn test_de_option_i32_valid() {
    let t: TestOptionI32 = serde_yaml::from_str("v: -42").unwrap();
    assert_eq!(t.v, Some(-42));

    let t: TestOptionI32 = serde_yaml::from_str("v: \"-5\"").unwrap();
    assert_eq!(t.v, Some(-5));
}

#[test]
fn test_de_option_i32_overflow() {
    let result: Result<TestOptionI32, _> =
        serde_yaml::from_str("v: 2147483648");
    assert!(result.is_err());
}

#[test]
fn test_de_option_i32_absent_is_none() {
    let t: TestOptionI32 = serde_yaml::from_str("{}").unwrap();
    assert_eq!(t.v, None);
}

#[test]
fn test_de_u8_or_string_valid() {
    let t: TestRequiredU8 = serde_yaml::from_str("v: 255").unwrap();
    assert_eq!(t.v, 255);

    let t: TestRequiredU8 = serde_yaml::from_str("v: \"200\"").unwrap();
    assert_eq!(t.v, 200);
}

#[test]
fn test_de_u8_or_string_missing_field() {
    let result: Result<TestRequiredU8, _> = serde_yaml::from_str("{}");
    assert!(result.is_err());
}

#[test]
fn test_de_u16_or_string_valid() {
    let t: TestRequiredU16 = serde_yaml::from_str("v: 65535").unwrap();
    assert_eq!(t.v, 65535);

    let t: TestRequiredU16 = serde_yaml::from_str("v: \"4096\"").unwrap();
    assert_eq!(t.v, 4096);
}

#[test]
fn test_de_u16_or_string_missing_field() {
    let result: Result<TestRequiredU16, _> = serde_yaml::from_str("{}");
    assert!(result.is_err());
}

#[test]
fn test_de_u32_or_string_valid() {
    let t: TestRequiredU32 = serde_yaml::from_str("v: 4294967295").unwrap();
    assert_eq!(t.v, u32::MAX);

    let t: TestRequiredU32 = serde_yaml::from_str("v: \"4096\"").unwrap();
    assert_eq!(t.v, 4096);
}

#[test]
fn test_de_u32_or_string_missing_field() {
    let result: Result<TestRequiredU32, _> = serde_yaml::from_str("{}");
    assert!(result.is_err());
}

