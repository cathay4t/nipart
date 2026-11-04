// SPDX-License-Identifier: Apache-2.0

use super::MergedDnsResolver;
use crate::{DnsResolver, DnsResolverClient};

#[test]
fn test_gen_state_for_apply_dedups_nameservers() {
    let merged = MergedDnsResolver {
        desired: None,
        current: DnsResolver::default(),
        running: None,
        servers: vec!["127.0.0.1".to_string()],
        searches: Vec::new(),
        search_defined: false,
        options: Vec::new(),
        dynamic_servers: vec!["127.0.0.1".to_string(), "192.0.2.1".to_string()],
        previous_static_servers: Vec::new(),
        cache_for_save: None,
    };

    let ret = merged.gen_state_for_apply();

    assert_eq!(
        ret.running,
        Some(DnsResolverClient {
            server: Some(vec![
                "127.0.0.1".to_string(),
                "192.0.2.1".to_string()
            ]),
            search: Some(Vec::new()),
            options: Some(Vec::new()),
        })
    );
}

#[test]
fn test_auto_search_allowed_when_search_undefined() {
    let merged = MergedDnsResolver {
        search_defined: false,
        searches: Vec::new(),
        ..Default::default()
    };
    assert!(merged.auto_search_allowed());
}

#[test]
fn test_auto_search_allowed_with_static_search() {
    let merged = MergedDnsResolver {
        search_defined: true,
        searches: vec!["example.org".to_string()],
        ..Default::default()
    };
    assert!(merged.auto_search_allowed());
}

#[test]
fn test_auto_search_suppressed_by_explicit_empty_search() {
    let merged = MergedDnsResolver {
        search_defined: true,
        searches: Vec::new(),
        ..Default::default()
    };
    assert!(!merged.auto_search_allowed());
}
