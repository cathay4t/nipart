// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, SocketAddr};

use nipart::{DnsCacheConfig, DnsUpstreamServer, NipartDnsUpstreamGroup};

/// Runtime configuration of the embedded DNS cache server.
///
/// It carries the schema level [DnsCacheConfig] plus the effective
/// nameservers resolved by the daemon (e.g. DHCP/RA learned servers for
/// `fallback.auto-dns: true`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NipartDnsServerConfig {
    pub(crate) bind: SocketAddr,
    pub(crate) max_cache_size: usize,
    pub(crate) load_etc_hosts: bool,
    /// Dynamic nameservers learned from DHCP/IPv6-RA/VPN. Kept separately
    /// so they can be refreshed without a full re-apply.
    pub(crate) auto_dns_servers: Vec<IpAddr>,
    pub(crate) fallback: DnsUpstreamConfig,
    pub(crate) doh: Option<DnsDohConfig>,
    pub(crate) groups: Vec<DnsGroupConfig>,
}

impl NipartDnsServerConfig {
    /// Build the runtime config from schema config.
    ///
    /// `auto_dns_servers` carries the dynamic nameservers learned from
    /// DHCP/IPv6-RA/VPN.  They are used only when
    /// `cache.fallback.auto-dns` is true and are placed before the static
    /// fallback nameservers so dynamic upstreams are preferred.
    pub(crate) fn new(
        cache: &DnsCacheConfig,
        auto_dns_servers: &[IpAddr],
    ) -> Result<Self, nipart::NipartError> {
        let bind = cache.bind_addr().ok_or_else(|| {
            nipart::NipartError::new(
                nipart::ErrorKind::InvalidArgument,
                format!("Invalid DNS cache bind address: {}", cache.bind),
            )
        })?;

        let fallback_servers = cache
            .fallback
            .nameservers
            .iter()
            .map(|srv| DnsUpstreamServer::parse(srv))
            .collect::<Result<Vec<_>, _>>()?;
        let auto_dns_servers = if cache.fallback.auto_dns {
            auto_dns_servers.to_vec()
        } else {
            Vec::new()
        };

        let groups = cache
            .groups
            .iter()
            .map(DnsGroupConfig::new)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            bind,
            max_cache_size: cache.max_cache_size,
            load_etc_hosts: cache.load_etc_hosts,
            auto_dns_servers,
            fallback: DnsUpstreamConfig {
                static_servers: fallback_servers,
                disable_ipv6: cache.fallback.disable_ipv6,
                blocking: false,
            },
            doh: cache.doh.as_ref().map(|doh| DnsDohConfig {
                nameservers: doh.nameservers.clone(),
                disable_ipv6: doh.disable_ipv6,
            }),
            groups,
        })
    }

    /// Whether response caching is enabled. `max-cache-size: 0` disables
    /// the cache while keeping the forwarding resolver running.
    pub(crate) fn cache_enabled(&self) -> bool {
        self.max_cache_size > 0
    }

    /// Replace the dynamic nameservers.  The fallback upstream list is
    /// rebuilt as dynamic servers first, then static ones.
    pub(crate) fn set_auto_dns_servers(&mut self, servers: &[IpAddr]) {
        self.auto_dns_servers = servers.to_vec();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DnsUpstreamConfig {
    /// Static upstream servers from schema config.
    pub(crate) static_servers: Vec<DnsUpstreamServer>,
    pub(crate) disable_ipv6: bool,
    /// An intentionally blocking upstream (group without nameservers)
    /// answers NXDOMAIN instead of being forwarded to the fallback.
    pub(crate) blocking: bool,
}

impl DnsUpstreamConfig {
    /// Effective upstream server list: dynamic servers first, then static
    /// ones. Groups only have static servers.
    pub(crate) fn servers(
        &self,
        auto_dns_servers: &[IpAddr],
    ) -> Vec<DnsUpstreamServer> {
        auto_dns_servers
            .iter()
            .copied()
            .map(|ip| DnsUpstreamServer::Ip(SocketAddr::new(ip, 53)))
            .chain(self.static_servers.iter().cloned())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DnsDohConfig {
    pub(crate) nameservers: Vec<IpAddr>,
    pub(crate) disable_ipv6: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DnsGroupConfig {
    pub(crate) name: String,
    pub(crate) domains: Vec<String>,
    pub(crate) upstream: DnsUpstreamConfig,
}

impl DnsGroupConfig {
    fn new(
        group: &NipartDnsUpstreamGroup,
    ) -> Result<Self, nipart::NipartError> {
        let servers = group
            .nameservers
            .iter()
            .map(|srv| DnsUpstreamServer::parse(srv))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            name: group.name.clone(),
            domains: group.domains.iter().map(|d| d.to_lowercase()).collect(),
            upstream: DnsUpstreamConfig {
                blocking: servers.is_empty(),
                static_servers: servers,
                disable_ipv6: group.disable_ipv6,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use nipart::DnsCacheConfig;

    use super::*;

    #[test]
    fn test_dynamic_auto_dns_prepended() {
        let mut cache = DnsCacheConfig::default();
        cache.fallback.nameservers = vec!["192.0.2.53".to_string()];
        let config = NipartDnsServerConfig::new(
            &cache,
            &["198.51.100.53".parse().unwrap()],
        )
        .unwrap();
        assert_eq!(
            config.fallback.servers(&config.auto_dns_servers),
            vec![
                DnsUpstreamServer::Ip("198.51.100.53:53".parse().unwrap()),
                DnsUpstreamServer::Ip("192.0.2.53:53".parse().unwrap()),
            ]
        );
    }

    #[test]
    fn test_auto_dns_disabled() {
        let mut cache = DnsCacheConfig::default();
        cache.fallback.auto_dns = false;
        let config = NipartDnsServerConfig::new(
            &cache,
            &["198.51.100.53".parse().unwrap()],
        )
        .unwrap();
        assert!(config.fallback.servers(&config.auto_dns_servers).is_empty());
    }
}
