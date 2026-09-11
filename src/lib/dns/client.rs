// SPDX-License-Identifier: Apache-2.0

use std::{
    net::{SocketAddr, UdpSocket},
    time::Duration,
};

use crate::{DnsHeader, DnsPacket, ErrorKind, NipartError};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// Whether a received datagram is the response to our query.
///
/// Pure decision helper used by [`DnsUdpClient::query`] so the
/// skip/accept rules (RFC 1035 §4.1.1 transaction ID and QR bit) can be
/// unit-tested without a socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DatagramVerdict {
    /// Unparseable or not a response to our query: keep waiting.
    Skip,
    /// The response to our query: parse and return it.
    Accept,
}

pub(crate) fn classify_response_datagram(
    expected_id: u16,
    datagram: &[u8],
) -> DatagramVerdict {
    if datagram.len() < DnsHeader::LEN {
        return DatagramVerdict::Skip;
    }
    let packet = match DnsPacket::parse(datagram) {
        Ok(packet) => packet,
        Err(_) => return DatagramVerdict::Skip,
    };
    if packet.header.id == expected_id && packet.header.qr {
        DatagramVerdict::Accept
    } else {
        DatagramVerdict::Skip
    }
}

/// A minimal blocking DNS-over-UDP client.
///
/// Binds an ephemeral local socket "connected" to a single server and
/// exchanges raw [`DnsPacket`]s. Intended for tests and simple tooling; the
/// daemon itself uses its own asynchronous transports.
pub struct DnsUdpClient {
    socket: UdpSocket,
}

impl DnsUdpClient {
    /// Create a client talking to `server`, given as `ip` or `ip:port`
    /// (port defaults to 53).
    pub fn new(server: &str) -> Result<Self, NipartError> {
        let addr: SocketAddr = if server.contains(':') {
            server.parse()
        } else {
            format!("{server}:53").parse()
        }
        .map_err(|e| {
            NipartError::new(
                ErrorKind::InvalidArgument,
                format!("Invalid DNS server address '{server}': {e}"),
            )
        })?;
        let bind_addr = if addr.is_ipv6() {
            "[::]:0"
        } else {
            "0.0.0.0:0"
        };
        let socket = UdpSocket::bind(bind_addr).map_err(|e| {
            NipartError::new(
                ErrorKind::Bug,
                format!("Failed to bind client UDP socket: {e}"),
            )
        })?;
        socket.connect(addr).map_err(|e| {
            NipartError::new(
                ErrorKind::Bug,
                format!("Failed to connect to DNS server {addr}: {e}"),
            )
        })?;
        socket
            .set_read_timeout(Some(DEFAULT_TIMEOUT))
            .map_err(|e| {
                NipartError::new(
                    ErrorKind::Bug,
                    format!("Failed to set client read timeout: {e}"),
                )
            })?;
        Ok(Self { socket })
    }

    /// Send `query` and wait for a single response, parsed into a
    /// [`DnsPacket`].
    ///
    /// Only a datagram whose transaction ID matches the query and whose QR
    /// bit is set is accepted; anything else (a stale response to an earlier
    /// query on this socket, or an unsolicited packet) is skipped.
    pub fn query(&self, query: &DnsPacket) -> Result<DnsPacket, NipartError> {
        let bytes = query.to_bytes();
        self.socket.send(&bytes).map_err(|e| {
            NipartError::new(
                ErrorKind::Bug,
                format!("Failed to send DNS query: {e}"),
            )
        })?;
        let deadline = std::time::Instant::now() + DEFAULT_TIMEOUT;
        let mut buf = [0u8; DnsPacket::MAX_UDP_EDNS_PACKET_SIZE];
        loop {
            let now = std::time::Instant::now();
            let Some(remaining) = deadline.checked_duration_since(now) else {
                return Err(NipartError::new(
                    ErrorKind::Timeout,
                    "Timed out waiting for DNS response".to_string(),
                ));
            };
            self.socket.set_read_timeout(Some(remaining)).map_err(|e| {
                NipartError::new(
                    ErrorKind::Bug,
                    format!("Failed to set client read timeout: {e}"),
                )
            })?;
            let n = match self.socket.recv(&mut buf) {
                Ok(n) => n,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Err(NipartError::new(
                        ErrorKind::Timeout,
                        "Timed out waiting for DNS response".to_string(),
                    ));
                }
                Err(e) => {
                    return Err(NipartError::new(
                        ErrorKind::Bug,
                        format!("Failed to receive DNS response: {e}"),
                    ));
                }
            };
            // Skip datagrams that are not a valid response to our query: a
            // mismatched transaction ID, a non-response, or garbage that
            // does not parse as DNS.
            match classify_response_datagram(query.header.id, &buf[..n]) {
                DatagramVerdict::Skip => continue,
                DatagramVerdict::Accept => {
                    return DnsPacket::parse(&buf[..n]).map_err(|e| {
                        NipartError::new(
                            ErrorKind::Bug,
                            format!(
                                "BUG: query response became unparseable after \
                                 validation: {e}"
                            ),
                        )
                    });
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "unit_tests/client_response.rs"]
mod client_response_tests;
