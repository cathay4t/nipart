// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;

use nipart::{DnsPacket, DnsResponseCode, DnsType};

use super::{classify_query, tcp_frame_reply};

#[test]
fn test_query_without_question_gets_formerr() {
    // A well-formed 12-byte header with qdcount = 0: not parseable into a
    // question, but a perfectly valid DNS packet otherwise.
    let query = vec![
        0x12, 0x34, // id
        0x01, 0x00, // flags: QR=0, opcode=0, RD=1
        0x00, 0x00, // qdcount = 0
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // an/ns/ar = 0
    ];
    let formerr = classify_query(&query, test_peer())
        .expect_err("query without question must be rejected");
    assert!(formerr.header.qr);
    assert_eq!(formerr.header.id, 0x1234);
    assert_eq!(formerr.header.rcode, DnsResponseCode::FormErr);
}

fn test_peer() -> SocketAddr {
    "127.0.0.1:12345".parse().expect("parse test peer")
}

#[test]
fn test_classify_query_short_buffer_yields_formerr() {
    let formerr = classify_query(&[0x12, 0x34, 0x01], test_peer())
        .expect_err("short buffer must be rejected");
    assert_eq!(formerr.header.id, 0x1234);
    assert_eq!(formerr.header.rcode, DnsResponseCode::FormErr);
}

#[test]
fn test_classify_query_ignores_responses() {
    let mut query =
        DnsPacket::new_query("example.com", DnsType::A).expect("query");
    query.header.qr = true; // a response, not a query
    let bytes = query.to_bytes();
    assert!(
        classify_query(&bytes, test_peer())
            .expect("classify")
            .is_none()
    );
}

#[test]
fn test_classify_query_accepts_valid_query() {
    let query = DnsPacket::new_query("example.com", DnsType::A).expect("query");
    let packet = classify_query(&query.to_bytes(), test_peer())
        .expect("classify")
        .expect("valid query must be accepted");
    assert!(packet.is_query());
    assert_eq!(
        packet.first_question().unwrap().domain.to_string(),
        "example.com"
    );
}

#[test]
fn test_tcp_framing_and_formerr() {
    // Query framing: a valid query frame carries the 2-byte length prefix
    // and the server accepts a second query on the same buffer
    // (RFC 7766 §6.2.1.1 connection reuse is covered by the integration
    // test; here we verify the pure framing rules).
    let query =
        DnsPacket::new_query("example.com", DnsType::A).expect("build query");
    let query_bytes = query.to_bytes();
    let frame = tcp_frame_reply(&query_bytes).expect("frame query");
    let len = u16::from_be_bytes([frame[0], frame[1]]) as usize;
    assert_eq!(len, query_bytes.len());
    assert_eq!(&frame[2..], query_bytes.as_slice());

    // A too-short frame is rejected by the query classifier, which
    // produces the framed FORMERR reply the connection task writes back.
    let formerr = classify_query(&[0x12, 0x34, 0x01, 0x00], test_peer())
        .expect_err("short frame must be rejected");
    assert_eq!(formerr.header.id, 0x1234);
    assert_eq!(formerr.header.rcode, DnsResponseCode::FormErr);
    let formerr_frame =
        tcp_frame_reply(&formerr.to_bytes()).expect("frame formerr");
    assert_eq!(
        u16::from_be_bytes([formerr_frame[0], formerr_frame[1]]) as usize,
        formerr_frame.len() - 2
    );

    // An oversized reply cannot be framed and must be rejected.
    let oversized = vec![0u8; u16::MAX as usize + 1];
    assert!(
        tcp_frame_reply(&oversized).is_err(),
        "a reply larger than u16::MAX must not be framed"
    );
}
