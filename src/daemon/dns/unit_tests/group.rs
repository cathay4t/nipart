// SPDX-License-Identifier: Apache-2.0

//! Pure unit tests for domain based upstream routing.
//!
//! No test creates a socket: transport creation goes through
//! [`FakeTransportFactory`], and the response demultiplexing rules are
//! driven by the pure register/dispatch helpers of [`DnsUdpTransport`].

use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use futures_util::future::BoxFuture;
use nipart::{
    DnsClass, DnsPacket, DnsResponseCode, DnsType, DnsUpstreamServer,
    NipartError,
};

use super::*;
use crate::dns::{
    config::{DnsGroupConfig, DnsUpstreamConfig, NipartDnsServerConfig},
    group::DnsTransportFactory,
};

fn test_addr() -> SocketAddr {
    "192.0.2.53:53".parse().expect("parse doc address")
}

#[derive(Default)]
struct FakeTransportPlan {
    /// How many `create()` calls fail before transports are created.
    fail_first: usize,
    /// When true, `send_query()` hangs until the test dispatches a
    /// response.
    hang: bool,
}

struct FakeTransportFactory {
    created: AtomicUsize,
    created_addrs: Mutex<Vec<SocketAddr>>,
    fail_first: usize,
    hang: bool,
}

impl FakeTransportFactory {
    fn new(plan: FakeTransportPlan) -> Arc<Self> {
        Arc::new(Self {
            created: AtomicUsize::new(0),
            created_addrs: Mutex::new(Vec::new()),
            fail_first: plan.fail_first,
            hang: plan.hang,
        })
    }

    fn created_count(&self) -> usize {
        self.created.load(Ordering::SeqCst)
    }

    fn created_addrs(&self) -> Vec<SocketAddr> {
        self.created_addrs.lock().unwrap().clone()
    }
}

impl DnsTransportFactory for FakeTransportFactory {
    fn create(
        &self,
        addr: SocketAddr,
        group_name: &str,
    ) -> BoxFuture<'static, Result<DnsUdpTransport, NipartError>> {
        if self.created.load(Ordering::SeqCst) < self.fail_first {
            self.created.fetch_add(1, Ordering::SeqCst);
            return Box::pin(async move {
                Err(NipartError::new(
                    nipart::ErrorKind::Bug,
                    "injected transport creation failure".to_string(),
                ))
            });
        }
        self.created.fetch_add(1, Ordering::SeqCst);
        self.created_addrs.lock().unwrap().push(addr);
        let group_name = group_name.to_string();
        let hang = self.hang;
        Box::pin(async move {
            Ok(DnsUdpTransport::new_fake(addr, &group_name, 0, hang))
        })
    }
}

fn test_config(fallback_ns: &str) -> NipartDnsServerConfig {
    NipartDnsServerConfig {
        bind: "127.0.0.1:53".parse().unwrap(),
        max_cache_size: 4096,
        load_etc_hosts: true,
        auto_dns_servers: Vec::new(),
        fallback: DnsUpstreamConfig {
            static_servers: vec![
                DnsUpstreamServer::parse(fallback_ns).unwrap(),
            ],
            disable_ipv6: false,
            blocking: false,
        },
        doh: None,
        groups: Vec::new(),
    }
}

/// A group whose upstream is the documentation address; tests inject a
/// fake factory so it never touches the network.
fn test_group() -> DnsGroup {
    DnsGroup::new(
        "test".to_string(),
        vec![DnsUpstreamServer::Ip(test_addr())],
        false,
        false,
        None,
        Arc::new(RealDnsTransportFactory),
    )
}

fn pending_key_for(query: &DnsPacket) -> (String, DnsType, DnsClass, u16) {
    let question = query.first_question().unwrap();
    (
        question.domain.to_string(),
        question.kind,
        question.class,
        query.header.id,
    )
}

#[tokio::test]
async fn test_transport_is_broken_when_recv_loop_panics() {
    // `recv_loop` only exits on a fatal socket error or a panic, so a
    // finished task handle is a reliable liveness signal even when the
    // panic path never ran `mark_broken`.
    let transport = DnsUdpTransport::new_fake(test_addr(), "test", 0, false);
    assert!(
        !transport.is_broken(),
        "a freshly created transport must be live"
    );

    transport.recv_task.abort();
    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !transport.is_broken() && std::time::Instant::now() < deadline {
        tokio::task::yield_now().await;
    }
    assert!(
        transport.is_broken(),
        "a transport whose receive loop exited silently must be detected as \
         broken so it is evicted and recreated"
    );
}

#[tokio::test]
async fn test_fallback_servfail_when_no_transports() {
    // The upstream never answers: the request must time out instead of
    // hanging, so the resolver can turn it into SERVFAIL.
    let factory = FakeTransportFactory::new(FakeTransportPlan {
        hang: true,
        ..Default::default()
    });
    let mut group = test_group();
    group.set_transport_factory(factory);
    group.set_request_timeout(Duration::from_millis(20));

    let query =
        DnsPacket::new_query("example.com", DnsType::A).expect("build query");
    let err = group
        .request(query)
        .await
        .expect_err("an unresponsive upstream must fail");
    // The request fails instead of hanging; the resolver turns the error
    // into SERVFAIL for every waiting client.
    assert!(
        matches!(
            err.kind(),
            nipart::ErrorKind::Timeout | nipart::ErrorKind::Bug
        ),
        "unexpected error kind: {err}"
    );
}

#[tokio::test]
async fn test_group_transports_created_on_demand() {
    let factory = FakeTransportFactory::new(FakeTransportPlan::default());
    let mut group = test_group();
    group.set_transport_factory(factory.clone());

    {
        let state = group.state.read().await;
        assert!(
            state.udp_transports.is_empty(),
            "no upstream transport should be created until first request"
        );
    }
    assert_eq!(factory.created_count(), 0);

    assert!(group.ensure_transports().await);
    let state = group.state.read().await;
    assert_eq!(state.udp_transports.len(), 1);
    assert_eq!(factory.created_count(), 1);
    assert_eq!(factory.created_addrs(), vec![test_addr()]);
}

#[tokio::test]
async fn test_named_group_resolves_when_fallback_unavailable() {
    // The named group must be selected for its domain, so the request is
    // routed to the corp group instead of the (unreachable) fallback.
    let config = {
        let mut config = test_config("192.0.2.53");
        config.groups.push(DnsGroupConfig {
            name: "corp".to_string(),
            domains: vec!["corp.example".to_string()],
            upstream: DnsUpstreamConfig {
                static_servers: Vec::new(),
                disable_ipv6: false,
                blocking: true,
            },
        });
        config
    };
    let groups = DnsGroups::new(config, None);
    let resp = groups
        .request(
            DnsPacket::new_query("host.corp.example", DnsType::A)
                .expect("build query"),
        )
        .await
        .expect("routed group must reply");
    // The corp group is blocking, which proves the request was routed to
    // it: the fallback would have tried the unreachable upstream instead.
    assert_eq!(resp.header.rcode, DnsResponseCode::NxDomain);
}

#[tokio::test]
async fn test_blocking_group_replies_nxdomain() {
    let mut config = test_config("192.0.2.53");
    config.groups.push(DnsGroupConfig {
        name: "blocked".to_string(),
        domains: vec!["blocked.example".to_string()],
        upstream: DnsUpstreamConfig {
            static_servers: Vec::new(),
            disable_ipv6: false,
            blocking: true,
        },
    });
    let groups = DnsGroups::new(config, None);
    let resp = groups
        .request(
            DnsPacket::new_query("host.blocked.example", DnsType::A)
                .expect("build query"),
        )
        .await
        .expect("blocking group must reply, not fail");
    assert_eq!(resp.header.rcode, DnsResponseCode::NxDomain);
}

#[tokio::test]
async fn test_longest_suffix_match_wins() {
    let mut config = test_config("192.0.2.53");
    config.groups.push(DnsGroupConfig {
        name: "broad".to_string(),
        domains: vec!["example.org".to_string()],
        upstream: DnsUpstreamConfig {
            static_servers: Vec::new(),
            disable_ipv6: false,
            blocking: true,
        },
    });
    config.groups.push(DnsGroupConfig {
        name: "narrow".to_string(),
        domains: vec!["internal.example.org".to_string()],
        upstream: DnsUpstreamConfig {
            static_servers: Vec::new(),
            disable_ipv6: false,
            blocking: true,
        },
    });
    let groups = DnsGroups::new(config, None);

    let narrow_key: Vec<String> = vec![
        "internal".to_string(),
        "example".to_string(),
        "org".to_string(),
    ];
    assert_eq!(
        groups.search_index.get(&narrow_key),
        Some(&"narrow".to_string())
    );
    let resp = groups
        .request(
            DnsPacket::new_query("host.internal.example.org", DnsType::A)
                .expect("build query"),
        )
        .await
        .expect("blocking group must reply");
    assert_eq!(resp.header.rcode, DnsResponseCode::NxDomain);
}

#[tokio::test]
async fn test_ensure_transports_recovers_after_cooldown() {
    let factory = FakeTransportFactory::new(FakeTransportPlan {
        fail_first: 1,
        ..Default::default()
    });
    let mut group = test_group();
    group.set_transport_factory(factory.clone());
    assert!(
        !group.ensure_transports().await,
        "a failed creation attempt must leave the group without transports"
    );
    assert!(
        !group.ensure_transports().await,
        "retry must be refused during the cooldown window"
    );
    assert_eq!(factory.created_count(), 1);

    group.recreate_gate.set_last_attempt(0);
    assert!(
        group.ensure_transports().await,
        "transports must be recreated after the cooldown"
    );
    let state = group.state.read().await;
    assert_eq!(state.udp_transports.len(), 1);
}

/// Concurrent queries for the same (domain, type, class) with the same
/// client transaction ID must never be cross-delivered: each query gets a
/// unique wire transaction ID and its own waiter.
#[tokio::test]
async fn test_concurrent_same_id_queries_not_cross_delivered() {
    let transport = DnsUdpTransport::new_fake(test_addr(), "test", 0, false);

    let mut query_do0 =
        DnsPacket::new_query("example.com", DnsType::A).unwrap();
    query_do0.add_opt_record(1232, false);
    let mut query_do1 =
        DnsPacket::new_query("example.com", DnsType::A).unwrap();
    query_do1.add_opt_record(1232, true);
    query_do1.header.id = query_do0.header.id;
    let client_id = query_do0.header.id;
    let key = pending_key_for(&query_do0);
    assert_eq!(key.3, client_id);

    let (bytes_do0, mut rx_do0) =
        transport.register_pending_for_test(&query_do0.to_bytes(), &key);
    let (bytes_do1, rx_do1) =
        transport.register_pending_for_test(&query_do1.to_bytes(), &key);
    assert_ne!(
        bytes_do0[0..2],
        bytes_do1[0..2],
        "each in-flight query must get a unique wire transaction ID"
    );

    // Deliver the DO=1 response first: only its waiter may receive it.
    let mut reply_do1 = DnsPacket::new_reply(
        u16::from_be_bytes([bytes_do1[0], bytes_do1[1]]),
        DnsResponseCode::NoError,
        query_do1.questions[0].domain.clone(),
        DnsType::A,
        DnsClass::IN,
        true,
    );
    reply_do1.add_opt_record(1232, true);
    assert_eq!(transport.dispatch_response_for_test(reply_do1), 1);
    assert!(
        rx_do0.try_recv().is_err(),
        "the DO=0 waiter must not receive the DO=1 response"
    );

    let mut reply_do0 = DnsPacket::new_reply(
        u16::from_be_bytes([bytes_do0[0], bytes_do0[1]]),
        DnsResponseCode::NoError,
        query_do0.questions[0].domain.clone(),
        DnsType::A,
        DnsClass::IN,
        true,
    );
    reply_do0.add_opt_record(1232, false);
    assert_eq!(transport.dispatch_response_for_test(reply_do0), 1);

    let resp0 = rx_do0.await.expect("DO=0 response");
    let resp1 = rx_do1.await.expect("DO=1 response");
    assert!(!resp0.dnssec_ok(), "DO=0 waiter got the DO=1 response");
    assert!(resp1.dnssec_ok(), "DO=1 waiter got the DO=0 response");
}
