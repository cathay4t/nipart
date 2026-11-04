// SPDX-License-Identifier: Apache-2.0

use nipart::{DnsCacheConfig, NipartError};

use super::{NipartDnsCmd, NipartDnsReply, NipartDnsServerConfig};
use crate::{TaskManager, dns_auto::AutoDnsSnapshot};

/// Manager of the embedded DNS cache task.
///
/// The worker owns the cache server. Applying a new cache configuration
/// stops the old server before starting the new one, so the bind port is
/// always released before the new server tries to bind it.
#[derive(Debug, Clone)]
pub(crate) struct NipartDnsManager {
    mgr: TaskManager<NipartDnsCmd, NipartDnsReply>,
}

impl NipartDnsManager {
    pub(crate) async fn new() -> Result<Self, NipartError> {
        Ok(Self {
            mgr: TaskManager::new::<super::NipartDnsWorker>("dns-cache")
                .await?,
        })
    }

    pub(crate) async fn shutdown(&self) {
        self.mgr.shutdown().await
    }

    /// Start, reconfigure or stop the DNS cache according to `cache`.
    ///
    /// `auto_dns` carries DNS information learned from DHCP/IPv6-RA/VPN
    /// used when `cache.auto-dns` is true.
    pub(crate) async fn apply_config(
        &mut self,
        cache: Option<&DnsCacheConfig>,
        auto_dns: &AutoDnsSnapshot,
    ) -> Result<(), NipartError> {
        let cache = match cache {
            Some(cache) => cache,
            None => {
                // `dns-resolver` not mentioned in desired state: keep the
                // current cache as is.
                return Ok(());
            }
        };
        if !cache.enabled {
            return self.stop().await;
        }
        let config = NipartDnsServerConfig::new(cache, auto_dns)?;
        self.mgr.exec(NipartDnsCmd::Start(config)).await?;
        Ok(())
    }

    /// Notify the running cache that the network path of its upstream
    /// nameservers changed, e.g. the default gateway was replaced by a
    /// route apply, a DHCP lease or the boot-up state restore.
    ///
    /// The cache keeps its cached replies but retries the upstream groups
    /// which were marked dead instead of waiting out their retry backoff.
    /// A stopped cache is not an error: a cache started later has no
    /// failure state to clear.
    pub(crate) async fn notify_network_change(
        &mut self,
    ) -> Result<(), NipartError> {
        self.mgr.exec(NipartDnsCmd::NotifyNetworkChange).await?;
        Ok(())
    }

    pub(crate) async fn stop(&mut self) -> Result<(), NipartError> {
        self.mgr.exec(NipartDnsCmd::Stop).await?;
        Ok(())
    }

    /// Whether the DNS cache server task is currently running.
    #[allow(dead_code)]
    pub(crate) async fn is_running(&mut self) -> Result<bool, NipartError> {
        Ok(self.query_config().await?.is_some())
    }

    /// Effective running cache configuration, including generated DHCP
    /// groups. `None` when no cache server is running.
    pub(crate) async fn query_config(
        &mut self,
    ) -> Result<Option<DnsCacheConfig>, NipartError> {
        match self.mgr.exec(NipartDnsCmd::Query).await? {
            NipartDnsReply::Running(config) => Ok(config),
            NipartDnsReply::None => Ok(None),
        }
    }
}
