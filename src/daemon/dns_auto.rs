// SPDX-License-Identifier: Apache-2.0

//! Resolve the `auto-dns` upstream nameservers for the DNS resolver and
//! the DNS cache.
//!
//! Nipart has no connection manager, so the dynamic DNS information is
//! collected from the running DHCPv4 workers. IPv6-RA learned nameservers
//! are not supported yet.

use std::collections::HashMap;

use nipart::{
    DnsResolverClient, InterfaceIpv4, MergedNetworkState, NetworkState,
    NipartDnsUpstreamGroup, NipartError, NipartInterface, NipartNoDaemon,
};

use super::{commander::NipartCommander, dhcp::NipartDhcpDnsInfo};

/// Effective DHCPv4 DNS information of one interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AutoDnsIface {
    pub iface: String,
    pub priority: u32,
    /// Position of the interface in the saved configuration. Interfaces
    /// without a saved entry sort after the saved ones.
    pub saved_order: usize,
    pub nameservers: Vec<String>,
    pub domains: Vec<String>,
    /// DHCPv4 option 119 domain search entries.
    pub domain_search: Vec<String>,
}

/// Sorted DHCPv4 DNS information used by the standard resolver and the
/// DNS cache.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AutoDnsSnapshot {
    pub ifaces: Vec<AutoDnsIface>,
    /// Deduplicated nameservers in effective order, without interface
    /// binding. Used by the standard resolver.
    pub servers: Vec<String>,
    /// Deduplicated nameservers bound to the interface that learned them.
    /// Used by the DNS cache fallback.
    pub cache_servers: Vec<String>,
    /// Deduplicated search domains in effective order.
    pub domains: Vec<String>,
    /// Whether any effective interface received DHCPv4 option 119.
    pub has_domain_search: bool,
    /// Cache groups generated from DHCP-provided domains.
    pub groups: Vec<NipartDnsUpstreamGroup>,
}

impl AutoDnsSnapshot {
    /// Warn when domain-based DNS splitting cannot work because the cache
    /// is disabled.
    pub(crate) fn warn_if_split_dns_unavailable(&self, cache_enabled: bool) {
        if !cache_enabled && self.has_domain_search {
            log::warn!(
                "DHCPv4 option 119 domain-search was received but the DNS                  cache is disabled: per-domain DNS splitting is not available"
            );
        }
    }
}

/// Build the effective auto-DNS snapshot.
///
/// `merged` is the state being applied when available; it overrides the
/// saved interface configuration. The periodic refresh passes `None` and
/// relies on `saved_state`.
pub(crate) fn collect_auto_dns(
    dhcp_info: &HashMap<String, NipartDhcpDnsInfo>,
    saved_state: &NetworkState,
    merged: Option<&MergedNetworkState>,
) -> AutoDnsSnapshot {
    let saved_order = saved_iface_order(saved_state);
    let mut ifaces: Vec<AutoDnsIface> = Vec::new();
    for (iface_name, info) in dhcp_info {
        let ipv4 = merged
            .and_then(|state| merged_ipv4(state, iface_name))
            .or_else(|| saved_ipv4(saved_state, iface_name));
        let auto_dns = ipv4.is_none_or(InterfaceIpv4::auto_dns_enabled);
        if !auto_dns {
            continue;
        }
        let priority = ipv4
            .map(InterfaceIpv4::auto_dns_priority_value)
            .unwrap_or(0);
        let mut nameservers: Vec<String> = Vec::new();
        for srv in &info.nameservers {
            if !nameservers.contains(srv) {
                nameservers.push(srv.clone());
            }
        }
        let mut domains: Vec<String> = Vec::new();
        for domain in &info.domains {
            let domain = domain.to_lowercase();
            if !domain.is_empty() && !domains.contains(&domain) {
                domains.push(domain);
            }
        }
        if nameservers.is_empty() && domains.is_empty() {
            continue;
        }
        let domain_search: Vec<String> = info
            .domain_search
            .iter()
            .map(|domain| domain.to_lowercase())
            .filter(|domain| !domain.is_empty())
            .collect();
        ifaces.push(AutoDnsIface {
            iface: iface_name.clone(),
            priority,
            saved_order: saved_order
                .get(iface_name.as_str())
                .copied()
                .unwrap_or(usize::MAX),
            nameservers,
            domains,
            domain_search,
        });
    }
    ifaces.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then(a.saved_order.cmp(&b.saved_order))
            .then(a.iface.cmp(&b.iface))
    });

    let mut servers: Vec<String> = Vec::new();
    let mut cache_servers: Vec<String> = Vec::new();
    let mut domains: Vec<String> = Vec::new();
    let mut has_domain_search = false;
    let mut groups: Vec<NipartDnsUpstreamGroup> = Vec::new();
    for iface in &ifaces {
        if !iface.domain_search.is_empty() {
            has_domain_search = true;
        }
        for srv in &iface.nameservers {
            if !servers.contains(srv) {
                servers.push(srv.clone());
            }
            let bound = format!(
                "{}:53%{}",
                srv.split('%').next().unwrap_or(srv),
                iface.iface
            );
            if !cache_servers.contains(&bound) {
                cache_servers.push(bound);
            }
        }
        for domain in &iface.domains {
            if !domains.contains(domain) {
                domains.push(domain.clone());
            }
        }
        if !iface.domains.is_empty() && !iface.nameservers.is_empty() {
            groups.push(gen_dhcp_group(iface));
        }
    }

    AutoDnsSnapshot {
        ifaces,
        servers,
        cache_servers,
        domains,
        has_domain_search,
        groups,
    }
}

fn gen_dhcp_group(iface: &AutoDnsIface) -> NipartDnsUpstreamGroup {
    let mut group = NipartDnsUpstreamGroup::default();
    group.name = format!("nipart-dhcp-{}", iface.iface);
    group.domains = iface.domains.clone();
    group.nameservers = iface
        .nameservers
        .iter()
        .map(|srv| {
            let ip = srv.split('%').next().unwrap_or(srv);
            format!("{ip}:53%{}", iface.iface)
        })
        .collect();
    group.from_dhcp = true;
    group
}

fn saved_iface_order(state: &NetworkState) -> HashMap<&str, usize> {
    state
        .ifaces
        .iter()
        .enumerate()
        .map(|(index, iface)| {
            (iface.base_iface().kernel_iface_name.as_str(), index)
        })
        .collect()
}

fn saved_ipv4<'a>(
    state: &'a NetworkState,
    iface_name: &str,
) -> Option<&'a InterfaceIpv4> {
    state
        .ifaces
        .kernel_ifaces
        .get(iface_name)
        .and_then(|iface| iface.base_iface().ipv4.as_ref())
}

fn merged_ipv4<'a>(
    state: &'a MergedNetworkState,
    iface_name: &str,
) -> Option<&'a InterfaceIpv4> {
    state.ifaces.iter().find_map(|merged_iface| {
        let base = merged_iface.merged.base_iface();
        (base.kernel_iface_name == iface_name || base.name == iface_name)
            .then_some(base.ipv4.as_ref())
            .flatten()
    })
}

impl NipartCommander {
    /// Collect the current auto-DNS snapshot.
    pub(crate) async fn auto_dns_snapshot(
        &mut self,
        saved_state: &NetworkState,
        merged: Option<&MergedNetworkState>,
    ) -> Result<AutoDnsSnapshot, NipartError> {
        let dhcp_info = self.dhcpv4_manager.dns_info().await?;
        Ok(collect_auto_dns(&dhcp_info, saved_state, merged))
    }

    /// Apply an auto-DNS snapshot using the saved resolver configuration.
    ///
    /// Used by the periodic refresh and the DHCP lease notification: the
    /// static DNS configuration is preserved while the dynamic entries of
    /// `/etc/resolv.conf` and the cache upstreams are updated.
    pub(crate) async fn apply_auto_dns_snapshot(
        &mut self,
        snapshot: &AutoDnsSnapshot,
    ) -> Result<(), NipartError> {
        let saved_state = self.conf_manager.query_state().await?;
        let resolver = saved_state.dns_resolver.clone();
        let config = resolver.config.clone().unwrap_or_default();
        let static_servers = config.server.clone().unwrap_or_default();
        let cache_enabled =
            resolver.cache.as_ref().is_some_and(|cache| cache.enabled);
        let cache_bind_ip = resolver
            .cache
            .as_ref()
            .filter(|cache| cache.enabled)
            .and_then(|cache| cache.bind_addr())
            .map(|addr| addr.ip());
        snapshot.warn_if_split_dns_unavailable(cache_enabled);
        let searches = config.search.clone().unwrap_or_default();
        let dynamic_searches = if auto_search_allowed(resolver.config.as_ref())
        {
            snapshot.domains.clone()
        } else {
            Vec::new()
        };
        NipartNoDaemon::apply_dns_resolver_conf(
            cache_bind_ip,
            &static_servers,
            &searches,
            &config.options.clone().unwrap_or_default(),
            &snapshot.servers,
            &dynamic_searches,
        )
        .await?;
        self.dns_manager
            .apply_config(resolver.cache.as_ref(), snapshot)
            .await
    }
}

/// Whether DHCP search domains may be appended to the static search list.
pub(crate) fn auto_search_allowed(config: Option<&DnsResolverClient>) -> bool {
    !config
        .and_then(|config| config.search.as_ref())
        .is_some_and(|search| search.is_empty())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use nipart::NetworkState;

    use super::*;

    fn dns_info(servers: &[&str], domains: &[&str]) -> NipartDhcpDnsInfo {
        NipartDhcpDnsInfo {
            nameservers: servers.iter().map(|s| s.to_string()).collect(),
            domains: domains.iter().map(|s| s.to_string()).collect(),
            domain_search: Vec::new(),
        }
    }

    #[test]
    fn test_auto_dns_priority_and_saved_order() {
        let saved: NetworkState = rmsd_yaml::from_str(
            r#"
            interfaces:
              - name: eth1
                type: ethernet
                state: up
                ipv4:
                  enabled: true
                  dhcp: true
                  auto-dns-priority: 10
              - name: eth2
                type: ethernet
                state: up
                ipv4:
                  enabled: true
                  dhcp: true
                  auto-dns-priority: 20
              - name: eth3
                type: ethernet
                state: up
                ipv4:
                  enabled: true
                  dhcp: true
                  auto-dns-priority: 10
            "#,
        )
        .unwrap();
        let mut dhcp_info = HashMap::new();
        dhcp_info.insert("eth1".to_string(), dns_info(&["192.0.2.1"], &[]));
        dhcp_info.insert("eth2".to_string(), dns_info(&["192.0.2.2"], &[]));
        dhcp_info.insert("eth3".to_string(), dns_info(&["192.0.2.3"], &[]));

        let snapshot = collect_auto_dns(&dhcp_info, &saved, None);
        assert_eq!(
            snapshot.servers,
            vec![
                "192.0.2.2".to_string(),
                "192.0.2.1".to_string(),
                "192.0.2.3".to_string(),
            ]
        );
    }

    #[test]
    fn test_auto_dns_false_is_ignored() {
        let saved: NetworkState = rmsd_yaml::from_str(
            r#"
            interfaces:
              - name: eth1
                type: ethernet
                state: up
                ipv4:
                  enabled: true
                  dhcp: true
                  auto-dns: false
            "#,
        )
        .unwrap();
        let mut dhcp_info = HashMap::new();
        dhcp_info.insert("eth1".to_string(), dns_info(&["192.0.2.1"], &[]));

        let snapshot = collect_auto_dns(&dhcp_info, &saved, None);
        assert!(snapshot.servers.is_empty());
    }

    #[test]
    fn test_domain_search_flag() {
        let saved: NetworkState = rmsd_yaml::from_str(
            r#"
            interfaces:
              - name: eth1
                type: ethernet
                state: up
                ipv4:
                  enabled: true
                  dhcp: true
            "#,
        )
        .unwrap();
        let mut dhcp_info = HashMap::new();
        let mut info = dns_info(&["192.0.2.1"], &["example.org"]);
        info.domain_search = vec!["example.org".to_string()];
        dhcp_info.insert("eth1".to_string(), info);

        let snapshot = collect_auto_dns(&dhcp_info, &saved, None);
        assert!(snapshot.has_domain_search);
    }

    #[test]
    fn test_dhcp_domain_group_uses_interface_binding() {
        let saved: NetworkState = rmsd_yaml::from_str(
            r#"
            interfaces:
              - name: eth1
                type: ethernet
                state: up
                ipv4:
                  enabled: true
                  dhcp: true
            "#,
        )
        .unwrap();
        let mut dhcp_info = HashMap::new();
        dhcp_info.insert(
            "eth1".to_string(),
            dns_info(&["192.0.2.1"], &["Example.Org"]),
        );

        let snapshot = collect_auto_dns(&dhcp_info, &saved, None);
        assert_eq!(snapshot.domains, vec!["example.org".to_string()]);
        assert_eq!(snapshot.groups.len(), 1);
        assert_eq!(snapshot.groups[0].name, "nipart-dhcp-eth1");
        assert_eq!(
            snapshot.groups[0].nameservers,
            vec!["192.0.2.1:53%eth1".to_string()]
        );
        assert!(snapshot.groups[0].from_dhcp);
    }
}
