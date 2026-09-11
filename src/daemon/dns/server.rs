// SPDX-License-Identifier: Apache-2.0

//! Embedded DNS cache server.
//!
//! Ported from the `mudz` project (same author, Apache-2.0). Instead of
//! being a standalone daemon with its own CLI/config file, the server is
//! started, reconfigured and stopped by the nipart daemon through
//! [`crate::dns::NipartDnsManager`].

use std::{net::SocketAddr, sync::Arc};

use futures_channel::{mpsc::unbounded, oneshot};
use nipart::{DnsPacket, ErrorKind, NipartError};
use tokio::net::{TcpListener, UdpSocket};

use super::{
    config::NipartDnsServerConfig,
    doh::{self, DohResolvCache},
    host::HostsFile,
    listener::{DnsTcpListener, DnsUdpListener},
    resolver::DnsResolver,
};

/// Where to deliver a resolved reply: back over the UDP socket that carried
/// the query, or over the client's TCP connection (RFC 1035 §4.2.2). TCP
/// replies are handed to the connection task through a oneshot, which frames
/// them with the 2-byte length prefix.
pub(crate) enum DnsReplyTarget {
    Udp(SocketAddr),
    Tcp {
        reply: oneshot::Sender<Vec<u8>>,
        peer: SocketAddr,
    },
}

impl DnsReplyTarget {
    pub(crate) fn is_tcp(&self) -> bool {
        matches!(self, DnsReplyTarget::Tcp { .. })
    }

    /// Deliver `buf` to the client. UDP replies are sent as one datagram;
    /// TCP replies are sent into the connection task's oneshot.
    pub(crate) async fn send(self, socket: &UdpSocket, buf: Vec<u8>) {
        match self {
            DnsReplyTarget::Udp(addr) => {
                if let Err(e) = socket.send_to(&buf, addr).await {
                    log::warn!("Failed to send DNS reply to {}: {e}", addr);
                }
            }
            DnsReplyTarget::Tcp { reply, peer } => {
                if reply.send(buf).is_err() {
                    log::debug!("TCP client {peer} closed before reply");
                }
            }
        }
    }
}

pub(crate) struct DnsQueryPacket {
    pub(crate) packet: DnsPacket,
    pub(crate) reply: DnsReplyTarget,
}

pub(crate) struct DnsCacheServer {
    socket: Arc<UdpSocket>,
    tcp_listener: Option<Arc<TcpListener>>,
    config: NipartDnsServerConfig,
    hosts: Arc<HostsFile>,
    /// DoH hostname-to-IP mapping resolved before the server starts. `None`
    /// when no DoH nameserver is configured.
    doh_cache: Option<Arc<DohResolvCache>>,
}

impl DnsCacheServer {
    pub(crate) async fn new(
        config: NipartDnsServerConfig,
    ) -> Result<Self, NipartError> {
        for group in &config.groups {
            log::info!(
                "Domain group '{}': {:?} -> {:?}",
                group.name,
                group.domains,
                group.upstream.static_servers,
            );
        }

        // DoH server hostnames are resolved once here, through the plain-IP
        // `doh.nameservers`, and pinned for the process lifetime. A failure
        // is fatal: the pinned-IP connector uses the mapping instead of the
        // system resolver, so no DoH query could ever succeed without
        // it.
        let hosts = Arc::new(if config.load_etc_hosts {
            HostsFile::new()
        } else {
            HostsFile::empty()
        });
        let doh_cache = doh::bootstrap_doh_cache(&config, &hosts).await?;

        let socket_addr = config.bind;
        let socket =
            Arc::new(UdpSocket::bind(&socket_addr).await.map_err(|e| {
                NipartError::new(
                    ErrorKind::InvalidArgument,
                    format!("Failed to bind UDP socket: {e}"),
                )
            })?);
        log::info!("DNS UDP server listening on {}", socket_addr);

        // DNS over TCP (RFC 7766): same address as UDP. A TCP bind failure is
        // not fatal — the daemon keeps serving UDP — but is logged loudly
        // because clients with truncated UDP replies (such as bind-utils
        // `host`) will fail their TCP fallback.
        let tcp_listener = match TcpListener::bind(&socket_addr).await {
            Ok(listener) => {
                log::info!("DNS TCP server listening on {}", socket_addr);
                Some(Arc::new(listener))
            }
            Err(e) => {
                log::warn!(
                    "Failed to bind TCP socket {socket_addr}: {e}; continuing \
                     UDP-only"
                );
                None
            }
        };

        Ok(Self {
            socket,
            tcp_listener,
            config,
            hosts,
            doh_cache,
        })
    }

    /// Run the server until `shutdown` resolves or a spawned task exits.
    pub(crate) async fn run_with_shutdown<F>(
        &self,
        shutdown: F,
    ) -> Result<(), NipartError>
    where
        F: std::future::Future<Output = ()> + Send,
    {
        let (sender, receiver) = unbounded::<DnsQueryPacket>();

        let socket = self.socket.clone();
        let udp_sender = sender.clone();
        let mut udp_listener_handle = tokio::spawn(async move {
            DnsUdpListener::run(udp_sender, socket).await
        });

        // The TCP listener shares the same query channel; replies are
        // routed back over each client's connection via `DnsReplyTarget`.
        let mut tcp_listener_handle =
            self.tcp_listener.as_ref().map(|listener| {
                let sender = sender.clone();
                let listener = Arc::clone(listener);
                tokio::spawn(async move {
                    DnsTcpListener::run(sender, listener).await
                })
            });

        let config = self.config.clone();
        let socket = self.socket.clone();
        let hosts = Arc::clone(&self.hosts);
        let doh_cache = self.doh_cache.clone();
        let mut resolver_handle = tokio::spawn(async move {
            DnsResolver::run(receiver, config, socket, hosts, doh_cache).await
        });

        tokio::select! {
            result = &mut udp_listener_handle => {
                match result {
                    Ok(()) => log::info!("DNS listener task exited"),
                    Err(e) => log::error!(
                        "DNS listener task panicked: {e}"
                    ),
                }
            }
            result = async {
                match &mut tcp_listener_handle {
                    Some(handle) => handle.await,
                    None => std::future::pending().await,
                }
            } => {
                match result {
                    Ok(()) => log::info!("DNS TCP listener task exited"),
                    Err(e) => log::error!(
                        "DNS TCP listener task panicked: {e}"
                    ),
                }
            }
            result = &mut resolver_handle => {
                match result {
                    Ok(()) => log::info!("DNS resolver task exited"),
                    Err(e) => log::error!(
                        "DNS resolver task panicked: {e}"
                    ),
                }
            }
            _ = shutdown => {
                log::info!("Receive shutdown signal");
            }
        }

        log::info!("Shutting down DNS cache server");
        // Abort the listener/resolver tasks and wait for them to finish so
        // their `Arc<UdpSocket>`/`Arc<TcpListener>` clones are dropped.  A
        // detached task would keep the bound port alive after the worker
        // dropped the server, making the next apply fail with EADDRINUSE.
        udp_listener_handle.abort();
        if let Some(handle) = tcp_listener_handle.as_ref() {
            handle.abort();
        }
        resolver_handle.abort();
        let _ = udp_listener_handle.await;
        if let Some(handle) = tcp_listener_handle {
            let _ = handle.await;
        }
        let _ = resolver_handle.await;
        Ok(())
    }
}
