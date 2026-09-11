// SPDX-License-Identifier: Apache-2.0

//! Standard DNS resolver: apply/query static DNS configuration through
//! `/etc/resolv.conf`.
//!
//! The dynamic nameservers learned from DHCP/IPv6-RA/VPN are kept as
//! `nameserver` entries without the nipart marker comment so they survive
//! a static configuration apply.  Nipart managed entries are written first
//! and marked with `# nipart: static`.

use std::net::IpAddr;

use crate::{
    DnsResolver, DnsResolverClient, ErrorKind, NipartError, NipartNoDaemon,
};

pub(crate) const RESOLV_CONF_PATH: &str = "/etc/resolv.conf";
const STATIC_MARKER: &str = "# nipart: static";

#[derive(Debug, Default)]
struct ResolvConf {
    /// `(is_nipart_static, server)` in file order.
    servers: Vec<(bool, String)>,
    /// `(is_nipart_static, search_domain)` in file order.  The keyword
    /// kind (`search` or `domain`) is normalized to a search entry.
    searches: Vec<(bool, String)>,
    /// `(is_nipart_static, option)` in file order.
    options: Vec<(bool, String)>,
}

impl ResolvConf {
    fn parse(content: &str) -> Self {
        let mut ret = Self::default();
        let mut is_static = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with('#') {
                if trimmed == STATIC_MARKER {
                    is_static = true;
                }
                continue;
            }
            let mut parts = trimmed.split_whitespace();
            let Some(kind) = parts.next() else {
                continue;
            };
            match kind {
                "nameserver" => {
                    if let Some(srv) = parts.next()
                        && srv.parse::<IpAddr>().is_ok()
                    {
                        ret.servers.push((is_static, srv.to_string()));
                    }
                }
                "search" | "domain" => {
                    for domain in parts {
                        ret.searches.push((is_static, domain.to_string()));
                    }
                }
                "options" => {
                    for opt in parts {
                        ret.options.push((is_static, opt.to_string()));
                    }
                }
                _ => (),
            }
        }
        ret
    }

    fn read() -> Result<Self, NipartError> {
        match std::fs::read_to_string(RESOLV_CONF_PATH) {
            Ok(content) => Ok(Self::parse(&content)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(Self::default())
            }
            Err(e) => Err(NipartError::new(
                ErrorKind::InvalidArgument,
                format!("Failed to read {RESOLV_CONF_PATH}: {e}"),
            )),
        }
    }

    fn dynamic_servers(&self) -> Vec<String> {
        self.dynamic_entries(&self.servers)
    }

    fn dynamic_searches(&self) -> Vec<String> {
        self.dynamic_entries(&self.searches)
    }

    fn dynamic_options(&self) -> Vec<String> {
        self.dynamic_entries(&self.options)
    }

    fn dynamic_entries(&self, entries: &[(bool, String)]) -> Vec<String> {
        entries
            .iter()
            .filter(|(is_static, _)| !is_static)
            .map(|(_, value)| value.clone())
            .collect()
    }

    /// Static entries of the file, used to preserve the previous static
    /// configuration when the desired `config` omits a property.
    #[allow(dead_code)]
    fn static_servers(&self) -> Vec<String> {
        self.static_entries(&self.servers)
    }

    #[allow(dead_code)]
    fn static_searches(&self) -> Vec<String> {
        self.static_entries(&self.searches)
    }

    #[allow(dead_code)]
    fn static_options(&self) -> Vec<String> {
        self.static_entries(&self.options)
    }

    fn static_entries(&self, entries: &[(bool, String)]) -> Vec<String> {
        entries
            .iter()
            .filter(|(is_static, _)| *is_static)
            .map(|(_, value)| value.clone())
            .collect()
    }
}

/// Query the running DNS resolver state from `/etc/resolv.conf`.
///
/// The `running` section carries the effective nameservers/search/options
/// currently in the file.  `config` section is not filled here; the daemon
/// merges the static part from its saved state.
pub(crate) fn query_running_dns() -> Result<DnsResolver, NipartError> {
    let conf = ResolvConf::read()?;
    let running = DnsResolverClient {
        server: Some(
            conf.servers
                .iter()
                .map(|(_, value)| value.clone())
                .collect(),
        ),
        search: Some(
            conf.searches
                .iter()
                .map(|(_, value)| value.clone())
                .collect(),
        ),
        options: Some(
            conf.options
                .iter()
                .map(|(_, value)| value.clone())
                .collect(),
        ),
    };
    Ok(DnsResolver {
        running: Some(running),
        config: None,
        cache: None,
    })
}

/// Write the static DNS configuration to `/etc/resolv.conf`, preserving
/// dynamic nameservers/domains already present in the file.
///
/// `cache_bind_ip` is the DNS cache listen address which must be written
/// first (when the cache is enabled).
pub(crate) fn write_static_dns(
    cache_bind_ip: Option<IpAddr>,
    static_servers: &[String],
    static_searches: &[String],
    static_options: &[String],
    dynamic_servers: &[String],
) -> Result<(), NipartError> {
    let existing = ResolvConf::read()?;
    let mut servers: Vec<String> = Vec::new();
    if let Some(bind_ip) = cache_bind_ip {
        // The cache must be the first nameserver so the host resolver
        // always queries it first (the same ordering is enforced by
        // `DnsResolver::validate()`).
        servers.push(bind_ip.to_string());
    }
    for srv in static_servers {
        if !servers.contains(srv) {
            servers.push(srv.clone());
        }
    }
    for srv in dynamic_servers {
        if !servers.contains(srv) {
            servers.push(srv.clone());
        }
    }
    for srv in existing.dynamic_servers() {
        if !servers.contains(&srv) {
            servers.push(srv);
        }
    }

    write_resolv_conf(&servers, static_searches, static_options)
}

/// Remove nipart managed static DNS configuration from `/etc/resolv.conf`.
pub(crate) fn purge_dns_resolver() -> Result<(), NipartError> {
    let existing = ResolvConf::read()?;
    let servers = existing.dynamic_servers();
    let searches = existing.dynamic_searches();
    let options = existing.dynamic_options();
    write_resolv_conf(&servers, &searches, &options)
}

fn write_resolv_conf(
    servers: &[String],
    searches: &[String],
    options: &[String],
) -> Result<(), NipartError> {
    let mut content = String::new();
    content.push_str(STATIC_MARKER);
    content.push('\n');
    for srv in servers {
        content.push_str(&format!("nameserver {srv}\n"));
    }
    if !searches.is_empty() {
        content.push_str(&format!("search {}\n", searches.join(" ")));
    }
    if !options.is_empty() {
        content.push_str(&format!("options {}\n", options.join(" ")));
    }

    let path = std::path::Path::new(RESOLV_CONF_PATH);
    // Write through a temporary file + rename.  `rename()` replaces the
    // symlink itself when resolv.conf is a symlink (e.g. to
    // systemd-resolved's stub file), exactly like nmstate's
    // `/etc/resolv.conf` handling.
    let tmp_path = path.with_extension("nipart.tmp");
    std::fs::write(&tmp_path, content).map_err(|e| {
        NipartError::new(
            ErrorKind::InvalidArgument,
            format!("Failed to write {}: {e}", tmp_path.display()),
        )
    })?;
    std::fs::rename(&tmp_path, path).map_err(|e| {
        NipartError::new(
            ErrorKind::InvalidArgument,
            format!("Failed to replace {RESOLV_CONF_PATH}: {e}"),
        )
    })
}

impl NipartNoDaemon {
    /// Query the running DNS resolver state from `/etc/resolv.conf`.
    pub async fn query_dns_resolver() -> Result<DnsResolver, NipartError> {
        query_running_dns()
    }

    /// Apply the static DNS resolver configuration to `/etc/resolv.conf`
    /// from explicit server/search/option lists.
    #[allow(clippy::too_many_arguments)]
    pub async fn apply_dns_resolver_conf(
        cache_bind_ip: Option<IpAddr>,
        static_servers: &[String],
        static_searches: &[String],
        static_options: &[String],
        dynamic_servers: &[String],
    ) -> Result<(), NipartError> {
        write_static_dns(
            cache_bind_ip,
            static_servers,
            static_searches,
            static_options,
            dynamic_servers,
        )
    }

    /// Purge nipart managed static DNS configuration.
    pub async fn purge_dns_resolver() -> Result<(), NipartError> {
        purge_dns_resolver()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NetworkState, NipartNoDaemon};

    #[test]
    fn test_parse_resolv_conf_mixed() {
        let content = "\
# Generated by NetworkManager
nameserver 192.0.2.1
search example.org
# nipart: static
nameserver 192.0.2.53
nameserver 2001:db8::53
search example.net
options ndots:2 trust-ad
";
        let conf = ResolvConf::parse(content);
        assert_eq!(conf.dynamic_servers(), vec!["192.0.2.1".to_string()]);
        assert_eq!(
            conf.static_servers(),
            vec!["192.0.2.53".to_string(), "2001:db8::53".to_string()]
        );
        assert_eq!(conf.static_searches(), vec!["example.net".to_string()]);
        assert_eq!(
            conf.static_options(),
            vec!["ndots:2".to_string(), "trust-ad".to_string()]
        );
    }

    #[test]
    fn test_parse_invalid_nameserver_ignored() {
        let conf =
            ResolvConf::parse("nameserver not-an-ip\nnameserver 192.0.2.1\n");
        assert_eq!(conf.servers.len(), 1);
        assert_eq!(conf.servers[0].1, "192.0.2.1");
    }

    #[tokio::test]
    async fn test_no_daemon_rejects_dns_cache() {
        // The cache is a daemon task: no-daemon mode must reject it
        // before touching `/etc/resolv.conf`.  Build the merged state
        // directly so the test performs no network or file access.
        let desired = NetworkState::new_from_yaml(
            r#"---
version: 1
dns-resolver:
  config:
    server:
      - 127.0.0.1
  cache:
    enabled: true
"#,
        )
        .unwrap();
        let merged_state = crate::MergedNetworkState::new(
            desired,
            NetworkState::default(),
            None,
            Default::default(),
        )
        .unwrap();
        let err = NipartNoDaemon::apply_dns_resolver(&merged_state)
            .await
            .expect_err("no-daemon mode must reject the DNS cache");
        assert_eq!(err.kind(), crate::ErrorKind::NoSupport);
    }
}
