// SPDX-License-Identifier: Apache-2.0

//! Minimal DNS wire format implementation used by the daemon DNS cache.
//!
//! The packet parsing/serialization code is ported from the `mudz` project
//! (same author, Apache-2.0). Only the wire format lives here; the cache
//! server itself is a daemon task.

mod client;
mod header;
mod packet;
mod record;

pub use self::{
    client::DnsUdpClient,
    header::{DnsHeader, DnsResponseCode},
    packet::{DnsPacket, DnsType},
    record::{
        DnsClass, DnsDomainName, DnsNameCompressionMap, DnsQuestion,
        DnsResourceRecord,
    },
};
