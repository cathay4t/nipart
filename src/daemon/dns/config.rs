// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
};

use mudz::{
    MudzConfig, MudzDohConfig, MudzFallbackConfig, MudzGroupConfig,
    MudzMainConfig,
};
use nipart::{
    DnsCacheConfig, DnsUpstreamServer, NipartDnsDoh, NipartDnsFallback,
    NipartDnsUpstreamGroup,
};

use crate::dns_auto::AutoDnsSnapshot;

/// Runtime configuration of the embedded DNS cache server.
///
/// It carries the schema level [DnsCacheConfig] plus the effective
/// nameservers resolved by the daemon (e.g. DHCP/RA learned servers for
/// `fallback.auto-dns: true`), and converts them into the [`MudzConfig`]
/// consumed by the embedded `mudz` server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NipartDnsServerConfig {
    pub(crate) bind: SocketAddr,
    pub(crate) max_cache_size: usize,
    pub(crate) load_etc_hosts: bool,
    /// Whether DHCP/IPv6-RA/VPN learned upstreams are enabled.
    pub(crate) auto_dns: bool,
    /// Dynamic nameservers learned from DHCP/IPv6-RA/VPN. Kept separately
    /// so they can be refreshed without a full re-apply.
    pub(crate) auto_dns_servers: Vec<DnsUpstreamServer>,
    pub(crate) fallback: DnsUpstreamConfig,
    pub(crate) doh: Option<DnsDohConfig>,
    /// Static groups from the schema configuration.
    pub(crate) static_groups: Vec<DnsGroupConfig>,
    /// Effective groups: static groups plus generated DHCP groups.
    pub(crate) groups: Vec<DnsGroupConfig>,
}

impl NipartDnsServerConfig {
    /// Build the runtime config from schema config.
    ///
    /// `auto_dns` carries the dynamic DNS information learned from
    /// DHCP/IPv6-RA/VPN. It is used only when `cache.auto-dns` is true.
    pub(crate) fn new(
        cache: &DnsCacheConfig,
        auto_dns: &AutoDnsSnapshot,
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
        let auto_dns_servers = if cache.auto_dns {
            auto_dns
                .cache_servers
                .iter()
                .map(|srv| DnsUpstreamServer::parse(srv))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        let empty_groups: Vec<NipartDnsUpstreamGroup> = Vec::new();
        let auto_groups = if cache.auto_dns {
            &auto_dns.groups
        } else {
            &empty_groups
        };

        let static_groups = cache
            .groups
            .iter()
            .map(DnsGroupConfig::new)
            .collect::<Result<Vec<_>, _>>()?;
        let groups = merge_groups(&static_groups, auto_groups)?;

        Ok(Self {
            bind,
            max_cache_size: cache.max_cache_size,
            load_etc_hosts: cache.load_etc_hosts,
            auto_dns: cache.auto_dns,
            auto_dns_servers,
            fallback: DnsUpstreamConfig {
                static_servers: fallback_servers,
                disable_ipv6: cache.fallback.disable_ipv6,
            },
            doh: cache.doh.as_ref().map(|doh| DnsDohConfig {
                nameservers: doh.nameservers.clone(),
                disable_ipv6: doh.disable_ipv6,
            }),
            static_groups,
            groups,
        })
    }

    /// Build the configuration of the embedded `mudz` server.
    ///
    /// The dynamic nameservers learned from DHCP/IPv6-RA/VPN are placed
    /// before the static fallback nameservers, so dynamic upstreams are
    /// preferred. `log_level` keeps its default because it only applies to
    /// the standalone `mudzd` binary; nipart installs its own logger.
    pub(crate) fn mudz_config(&self) -> MudzConfig {
        let mut groups = HashMap::with_capacity(self.groups.len());
        for group in &self.groups {
            groups.insert(
                group.name.clone(),
                MudzGroupConfig::new(
                    // A group without nameservers means "reply NXDOMAIN"
                    // to mudz as well.
                    group
                        .upstream
                        .static_servers
                        .iter()
                        .map(upstream_server_string)
                        .collect(),
                    group.domains.clone(),
                    group.upstream.disable_ipv6,
                ),
            );
        }

        // TCP listens on the same address, like the daemon did before:
        // `tcp_bind` defaults to `None` in mudz. `log_level` keeps its
        // default too since it only applies to the standalone `mudzd`
        // binary; nipart installs its own logger.
        let mut main = MudzMainConfig::default();
        main.udp_bind = self.bind.to_string();
        main.max_cache_size = self.max_cache_size;
        main.load_etc_hosts = self.load_etc_hosts;

        let fallback = MudzFallbackConfig::new(
            self.fallback
                .servers(&self.auto_dns_servers)
                .iter()
                .map(upstream_server_string)
                .collect(),
            self.fallback.disable_ipv6,
        );

        let doh = self.doh.as_ref().map(|doh| {
            let mut doh_config = MudzDohConfig::default();
            doh_config.nameservers = doh.nameservers.clone();
            doh_config.disable_ipv6 = doh.disable_ipv6;
            doh_config
        });

        MudzConfig::new(main, fallback, doh, groups)
    }

    /// The effective cache configuration for querying.
    pub(crate) fn to_schema_config(&self) -> DnsCacheConfig {
        let mut fallback = NipartDnsFallback::default();
        fallback.nameservers = self
            .fallback
            .static_servers
            .iter()
            .map(upstream_server_string)
            .collect();
        fallback.disable_ipv6 = self.fallback.disable_ipv6;
        let doh = self.doh.as_ref().map(|doh| {
            let mut config = NipartDnsDoh::default();
            config.nameservers = doh.nameservers.clone();
            config.disable_ipv6 = doh.disable_ipv6;
            config
        });
        let groups = self
            .groups
            .iter()
            .map(|group| {
                let mut schema_group = NipartDnsUpstreamGroup::default();
                schema_group.name = group.name.clone();
                schema_group.domains = group.domains.clone();
                schema_group.nameservers = group
                    .upstream
                    .static_servers
                    .iter()
                    .map(upstream_server_string)
                    .collect();
                schema_group.disable_ipv6 = group.upstream.disable_ipv6;
                schema_group.from_dhcp = group.from_dhcp;
                schema_group
            })
            .collect();
        let mut config = DnsCacheConfig::default();
        config.enabled = true;
        config.bind = self.bind.to_string();
        config.max_cache_size = self.max_cache_size;
        config.load_etc_hosts = self.load_etc_hosts;
        config.auto_dns = self.auto_dns;
        config.fallback = fallback;
        config.doh = doh;
        config.groups = groups;
        config
    }
}

/// Nameserver string accepted by mudz: plain IP, `IP:port` or DoH URL.
fn upstream_server_string(server: &DnsUpstreamServer) -> String {
    server.to_string()
}

/// Merge generated DHCP groups into the static groups.
///
/// An auto group whose domain is already covered by a static group has its
/// nameservers appended after the static ones. Other auto groups are added
/// as new runtime groups. If several auto groups share a domain they are
/// merged into a single group.
fn merge_groups(
    static_groups: &[DnsGroupConfig],
    auto_groups: &[NipartDnsUpstreamGroup],
) -> Result<Vec<DnsGroupConfig>, nipart::NipartError> {
    let mut ret = static_groups.to_vec();
    for auto_group in auto_groups {
        let auto_domains: Vec<String> = auto_group
            .domains
            .iter()
            .map(|domain| domain.to_lowercase())
            .collect();
        let auto_servers = auto_group
            .nameservers
            .iter()
            .map(|srv| DnsUpstreamServer::parse(srv))
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(existing) = ret.iter_mut().find(|group| {
            group
                .domains
                .iter()
                .any(|domain| auto_domains.contains(domain))
        }) {
            for srv in auto_servers {
                if !existing.upstream.static_servers.contains(&srv) {
                    existing.upstream.static_servers.push(srv);
                }
            }
            for domain in auto_domains {
                if !existing.domains.contains(&domain) {
                    existing.domains.push(domain);
                }
            }
            continue;
        }
        if let Some(existing) =
            ret.iter_mut().find(|group| group.name == auto_group.name)
        {
            for srv in auto_servers {
                if !existing.upstream.static_servers.contains(&srv) {
                    existing.upstream.static_servers.push(srv);
                }
            }
            for domain in auto_domains {
                if !existing.domains.contains(&domain) {
                    existing.domains.push(domain);
                }
            }
            continue;
        }
        ret.push(DnsGroupConfig {
            name: auto_group.name.clone(),
            domains: auto_domains,
            upstream: DnsUpstreamConfig {
                static_servers: auto_servers,
                disable_ipv6: auto_group.disable_ipv6,
            },
            from_dhcp: true,
        });
    }
    Ok(ret)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DnsUpstreamConfig {
    /// Static upstream servers from schema config.
    pub(crate) static_servers: Vec<DnsUpstreamServer>,
    pub(crate) disable_ipv6: bool,
}

impl DnsUpstreamConfig {
    /// Effective upstream server list: static servers first, then dynamic
    /// ones.
    pub(crate) fn servers(
        &self,
        auto_dns_servers: &[DnsUpstreamServer],
    ) -> Vec<DnsUpstreamServer> {
        self.static_servers
            .iter()
            .cloned()
            .chain(auto_dns_servers.iter().cloned())
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
    pub(crate) from_dhcp: bool,
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
                static_servers: servers,
                disable_ipv6: group.disable_ipv6,
            },
            from_dhcp: group.from_dhcp,
        })
    }
}

#[cfg(test)]
mod tests {
    use nipart::DnsCacheConfig;

    use super::*;

    fn snapshot(servers: &[&str]) -> AutoDnsSnapshot {
        AutoDnsSnapshot {
            servers: servers.iter().map(|srv| srv.to_string()).collect(),
            cache_servers: servers.iter().map(|srv| srv.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn test_static_auto_dns_first() {
        let mut cache = DnsCacheConfig::default();
        cache.fallback.nameservers = vec!["192.0.2.53".to_string()];
        let config =
            NipartDnsServerConfig::new(&cache, &snapshot(&["198.51.100.53"]))
                .unwrap();
        assert_eq!(
            config.fallback.servers(&config.auto_dns_servers),
            vec![
                DnsUpstreamServer::from_ip("192.0.2.53".parse().unwrap()),
                DnsUpstreamServer::from_ip("198.51.100.53".parse().unwrap()),
            ]
        );
    }

    #[test]
    fn test_auto_dns_disabled() {
        let mut cache = DnsCacheConfig::default();
        cache.auto_dns = false;
        let config =
            NipartDnsServerConfig::new(&cache, &snapshot(&["198.51.100.53"]))
                .unwrap();
        assert!(config.fallback.servers(&config.auto_dns_servers).is_empty());
    }

    fn snapshot_with_group(
        servers: &[&str],
        name: &str,
        domains: &[&str],
        group_servers: &[&str],
    ) -> AutoDnsSnapshot {
        let mut group = nipart::NipartDnsUpstreamGroup::default();
        group.name = name.to_string();
        group.domains = domains.iter().map(|d| d.to_string()).collect();
        group.nameservers =
            group_servers.iter().map(|s| s.to_string()).collect();
        group.from_dhcp = true;
        AutoDnsSnapshot {
            servers: servers.iter().map(|s| s.to_string()).collect(),
            cache_servers: servers.iter().map(|s| s.to_string()).collect(),
            groups: vec![group],
            ..Default::default()
        }
    }

    #[test]
    fn test_generated_group_is_runtime_only() {
        let mut cache = DnsCacheConfig::default();
        cache.enabled = true;
        cache.bind = "127.0.0.1:5353".to_string();
        let config = NipartDnsServerConfig::new(
            &cache,
            &snapshot_with_group(
                &["198.51.100.53"],
                "nipart-dhcp-eth1",
                &["example.org"],
                &["198.51.100.53:53%eth1"],
            ),
        )
        .unwrap();
        let query = config.to_schema_config();
        let group = query
            .groups
            .iter()
            .find(|group| group.name == "nipart-dhcp-eth1")
            .unwrap();
        assert!(group.from_dhcp);
        assert_eq!(
            group.nameservers,
            vec!["198.51.100.53:53%eth1".to_string()]
        );
    }

    #[test]
    fn test_generated_group_appends_after_static_domain() {
        let mut cache = DnsCacheConfig::default();
        cache.enabled = true;
        cache.bind = "127.0.0.1:5353".to_string();
        let mut home = nipart::NipartDnsUpstreamGroup::default();
        home.name = "home".to_string();
        home.domains = vec!["example.org".to_string()];
        home.nameservers = vec!["192.0.2.53".to_string()];
        cache.groups = vec![home];
        let config = NipartDnsServerConfig::new(
            &cache,
            &snapshot_with_group(
                &["198.51.100.53"],
                "nipart-dhcp-eth1",
                &["example.org"],
                &["198.51.100.53:53%eth1"],
            ),
        )
        .unwrap();
        let groups = config.to_schema_config().groups;
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "home");
        assert_eq!(
            groups[0].nameservers,
            vec![
                "192.0.2.53".to_string(),
                "198.51.100.53:53%eth1".to_string(),
            ]
        );
    }

    /// A port written in the configuration pins every mudz attempt,
    /// including the DoT probe, while a bare address is handed over
    /// without a port so mudz can probe DoT on 853.
    #[test]
    fn test_nameserver_port_preserved_for_mudz() {
        let mut cache = DnsCacheConfig::default();
        let mut home = nipart::NipartDnsUpstreamGroup::default();
        home.name = "home".to_string();
        home.nameservers = vec![
            "192.0.2.53".to_string(),
            "192.0.2.54:5353".to_string(),
            "2001:db8::53".to_string(),
        ];
        cache.groups = vec![home];

        let config =
            NipartDnsServerConfig::new(&cache, &snapshot(&[])).unwrap();
        let mudz_config = config.mudz_config();

        assert_eq!(
            mudz_config.groups.get("home").unwrap().nameservers,
            vec![
                "192.0.2.53".to_string(),
                "192.0.2.54:5353".to_string(),
                "2001:db8::53".to_string(),
            ]
        );
    }

    #[test]
    fn test_mudz_config_maps_schema() {
        let mut cache = DnsCacheConfig::default();
        cache.enabled = true;
        cache.bind = "127.0.0.1:5353".to_string();
        cache.max_cache_size = 0;
        cache.load_etc_hosts = false;
        cache.fallback.nameservers =
            vec!["https://dns.example.org/dns-query".to_string()];
        cache.fallback.disable_ipv6 = true;
        cache.doh = Some(Default::default());
        cache.doh.as_mut().unwrap().nameservers =
            vec!["192.0.2.53".parse().unwrap()];
        let mut home = nipart::NipartDnsUpstreamGroup::default();
        home.name = "home".to_string();
        home.domains = vec!["Sweat.Example.Org".to_string()];
        home.nameservers = vec!["192.0.2.1".to_string()];
        home.disable_ipv6 = true;
        let mut blocked = nipart::NipartDnsUpstreamGroup::default();
        blocked.name = "blocked".to_string();
        blocked.domains = vec!["ads.example".to_string()];
        cache.groups = vec![home, blocked];

        let config =
            NipartDnsServerConfig::new(&cache, &snapshot(&["198.51.100.53"]))
                .unwrap();
        let mudz_config = config.mudz_config();

        assert_eq!(mudz_config.main.udp_bind, "127.0.0.1:5353");
        assert_eq!(mudz_config.main.max_cache_size, 0);
        assert!(!mudz_config.main.load_etc_hosts);
        // Static nameservers come before the dynamic ones.
        assert_eq!(
            mudz_config.fallback.nameservers,
            vec![
                "https://dns.example.org/dns-query".to_string(),
                // Implicit port: mudz probes DoT on 853 and falls back to
                // plaintext on 53.
                "198.51.100.53".to_string(),
            ]
        );
        assert!(mudz_config.fallback.disable_ipv6);
        assert_eq!(
            mudz_config.doh.as_ref().unwrap().nameservers,
            vec!["192.0.2.53".parse::<IpAddr>().unwrap()]
        );

        let home = mudz_config.groups.get("home").unwrap();
        assert_eq!(home.nameservers, vec!["192.0.2.1".to_string()]);
        assert_eq!(home.domains, vec!["sweat.example.org".to_string()]);
        assert!(home.disable_ipv6);
        // A group without nameservers means "reply NXDOMAIN" in mudz too.
        assert!(
            mudz_config
                .groups
                .get("blocked")
                .unwrap()
                .nameservers
                .is_empty()
        );

        mudz_config.validate().unwrap();
    }
}
