// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4},
};

use hyper::Uri;
use nipart::{DnsUpstreamServer, ErrorKind};

use super::*;
use crate::dns::config::{DnsDohConfig, DnsGroupConfig, DnsUpstreamConfig};

/// A runtime config carrying no upstream nameserver: `bootstrap_doh_cache`
/// only looks at the DoH parts.
fn test_config(
    fallback_servers: Vec<DnsUpstreamServer>,
    groups: Vec<DnsGroupConfig>,
    doh: Option<DnsDohConfig>,
) -> NipartDnsServerConfig {
    NipartDnsServerConfig {
        bind: "127.0.0.1:53".parse().unwrap(),
        max_cache_size: 4096,
        load_etc_hosts: true,
        auto_dns_servers: Vec::new(),
        fallback: DnsUpstreamConfig {
            static_servers: fallback_servers,
            disable_ipv6: false,
            blocking: false,
        },
        doh,
        groups,
    }
}

/// The nameserver address used in pure resolution tests.  No socket is
/// ever created: the injected transport closure ignores it.
fn fake_nameserver_addr() -> SocketAddr {
    "192.0.2.53:53".parse().expect("parse doc address")
}

#[tokio::test]
async fn test_resolve_hostname_uses_bootstrap_nameserver() {
    let expected = Ipv4Addr::new(192, 0, 2, 3);
    let ips = resolve_hostname_with(
        "doh-bootstrap.example.org",
        &vec![fake_nameserver_addr()],
        true,
        &HostsFile::empty(),
        |_nss, query| {
            assert_eq!(query.first_question().unwrap().kind, DnsType::A);
            async move { Ok(vec![IpAddr::V4(expected)]) }.boxed()
        },
    )
    .await
    .expect("bootstrap resolution must succeed");
    assert_eq!(ips, vec![IpAddr::V4(expected)]);
}

#[tokio::test]
async fn test_resolve_hostname_fails_when_bootstrap_fails() {
    let err = resolve_hostname_with(
        "doh-bootstrap.example.org",
        &vec![fake_nameserver_addr()],
        true,
        &HostsFile::empty(),
        |_nss, _query| {
            async move {
                Err(NipartError::new(
                    ErrorKind::Timeout,
                    "bootstrap timeout".to_string(),
                ))
            }
            .boxed()
        },
    )
    .await
    .expect_err("a failing bootstrap nameserver must abort resolution");
    assert!(
        err.to_string().contains("doh-bootstrap.example.org"),
        "error must identify the hostname: {err}"
    );
}

#[tokio::test]
async fn test_resolve_hostname_hosts_file_wins() {
    let mut hosts = HostsFile::empty();
    hosts.insert_a_for_test(
        "doh-bootstrap.example.org",
        Ipv4Addr::new(198, 51, 100, 7),
    );
    let ips = resolve_hostname_with(
        "doh-bootstrap.example.org",
        &vec![fake_nameserver_addr()],
        true,
        &hosts,
        |_nss, _query| {
            async move { panic!("hosts file hit must not query nameservers") }
                .boxed()
        },
    )
    .await
    .expect("hosts file resolution must succeed");
    assert_eq!(ips, vec![IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7))]);
}

#[tokio::test]
async fn test_resolve_hostname_a_then_aaaa_by_default() {
    let queries = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let queries_for_transport = queries.clone();
    let ips = resolve_hostname_with(
        "doh-bootstrap.example.org",
        &vec![fake_nameserver_addr()],
        false,
        &HostsFile::empty(),
        |_nss, query| {
            let kind = query.first_question().unwrap().kind;
            queries_for_transport.lock().unwrap().push(kind);
            async move {
                if kind == DnsType::A {
                    Ok(vec![IpAddr::V4(Ipv4Addr::new(192, 0, 2, 3))])
                } else {
                    Ok(vec![IpAddr::V6("2001:db8::3".parse().unwrap())])
                }
            }
            .boxed()
        },
    )
    .await
    .expect("bootstrap resolution must succeed");
    assert_eq!(*queries.lock().unwrap(), vec![DnsType::A, DnsType::AAAA]);
    assert_eq!(ips.len(), 2);
}

#[test]
fn test_doh_hostnames_are_unique_and_lowercased() {
    let config = test_config(
        vec![
            DnsUpstreamServer::parse("https://doh.example.org/dns-query")
                .unwrap(),
            DnsUpstreamServer::parse("https://dns.example.com/dns-query")
                .unwrap(),
        ],
        vec![DnsGroupConfig {
            name: "corp".to_string(),
            domains: Vec::new(),
            upstream: DnsUpstreamConfig {
                static_servers: vec![
                    DnsUpstreamServer::parse(
                        "https://DNS.Example.COM/dns-query",
                    )
                    .unwrap(),
                    DnsUpstreamServer::parse("192.0.2.1").unwrap(),
                ],
                disable_ipv6: false,
                blocking: false,
            },
        }],
        None,
    );

    let hostnames: Vec<String> =
        doh_hostnames(&config).unwrap().into_iter().collect();
    assert_eq!(
        hostnames,
        vec!["dns.example.com".to_string(), "doh.example.org".to_string()]
    );
}

#[tokio::test]
async fn test_bootstrap_without_doh_is_none() {
    let config = test_config(Vec::new(), Vec::new(), None);
    let cache = bootstrap_doh_cache(&config, &HostsFile::empty())
        .await
        .expect("no DoH configured must not fail");
    assert!(cache.is_none());
}

#[tokio::test]
async fn test_bootstrap_without_doh_section_fails() {
    let config = test_config(
        vec![
            DnsUpstreamServer::parse("https://dns.example.com/dns-query")
                .unwrap(),
        ],
        Vec::new(),
        None,
    );
    let err = bootstrap_doh_cache(&config, &HostsFile::empty())
        .await
        .err()
        .expect("DoH without plain-IP nameservers must fail startup");
    assert!(
        err.to_string().contains("doh"),
        "error must mention the missing DoH nameservers: {err}"
    );
}

#[test]
fn test_pinned_connector_rejects_unknown_hostname() {
    let pinned = HashMap::new();
    let uri = Uri::from_static("https://unknown.test/dns-query");
    let err = pinned_connection_target(&uri, &pinned)
        .expect_err("hostname outside the pinned registry must not connect");
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn test_pinned_connector_uses_pinned_ip_and_uri_port() {
    let mut pinned = HashMap::new();
    pinned.insert(
        "doh.test".to_string(),
        vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
    );
    // Mixed-case hostname and non-default port: the connector must match
    // the pinned entry case-insensitively and use the port from the URI.
    let uri: Uri = "https://DoH.test:8443/dns-query".parse().unwrap();
    let (host, addrs) =
        pinned_connection_target(&uri, &pinned).expect("pinned hostname");
    assert_eq!(host, "doh.test");
    assert_eq!(
        addrs,
        vec![SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8443))]
    );
}

#[test]
fn test_pinned_connector_defaults_to_https_port() {
    let mut pinned = HashMap::new();
    pinned.insert(
        "doh.test".to_string(),
        vec![
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
            IpAddr::V6("2001:db8::1".parse().unwrap()),
        ],
    );
    let uri = Uri::from_static("https://doh.test/dns-query");
    let (_, addrs) =
        pinned_connection_target(&uri, &pinned).expect("pinned hostname");
    assert_eq!(addrs.len(), 2);
    assert!(addrs.iter().all(|addr| addr.port() == 443));
}
