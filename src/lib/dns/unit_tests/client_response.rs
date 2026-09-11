// SPDX-License-Identifier: Apache-2.0

//! Pure tests of the DNS-over-UDP response classification.
//!
//! No socket is created: the client's wait loop is driven by
//! [`classify_response_datagram`], which decides whether a received
//! datagram answers our query.

use crate::{
    DnsClass, DnsHeader, DnsPacket, DnsResponseCode, DnsType,
    dns::client::{DatagramVerdict, classify_response_datagram},
};

fn reply_bytes(transaction_id: u16, is_response: bool) -> Vec<u8> {
    let query = DnsPacket::new_query("example.com", DnsType::A).unwrap();
    let mut reply = DnsPacket::new_reply(
        transaction_id,
        DnsResponseCode::NoError,
        query.questions[0].domain.clone(),
        DnsType::A,
        DnsClass::IN,
        true,
    );
    reply.header.qr = is_response;
    reply.to_bytes()
}

#[test]
fn test_matching_response_is_accepted() {
    let datagram = reply_bytes(0x1234, true);
    assert_eq!(
        classify_response_datagram(0x1234, &datagram),
        DatagramVerdict::Accept
    );
}

#[test]
fn test_mismatched_transaction_id_is_skipped() {
    let datagram = reply_bytes(0x9999, true);
    assert_eq!(
        classify_response_datagram(0x1234, &datagram),
        DatagramVerdict::Skip
    );
}

#[test]
fn test_non_response_with_matching_id_is_skipped() {
    // A query (QR=0) echoing our transaction ID must not be accepted as a
    // response.
    let mut query = DnsPacket::new_query("example.com", DnsType::A).unwrap();
    query.header.id = 0x1234;
    assert_eq!(
        classify_response_datagram(0x1234, &query.to_bytes()),
        DatagramVerdict::Skip
    );
}

#[test]
fn test_unparseable_datagram_is_skipped() {
    // Long enough to have a header, but the header claims records that are
    // not present.
    let mut garbage = vec![0u8; DnsHeader::LEN];
    garbage[0] = 0x12;
    garbage[1] = 0x34;
    garbage[2] = 0x81;
    garbage[3] = 0x80;
    garbage[5] = 1; // qdcount = 1 without a question section
    assert_eq!(
        classify_response_datagram(0x1234, &garbage),
        DatagramVerdict::Skip
    );
}

#[test]
fn test_short_datagram_is_skipped() {
    assert_eq!(
        classify_response_datagram(0x1234, &[0x12, 0x34]),
        DatagramVerdict::Skip
    );
    assert_eq!(
        classify_response_datagram(0x1234, &[]),
        DatagramVerdict::Skip
    );
}
