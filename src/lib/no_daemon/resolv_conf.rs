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
const DYNAMIC_MARKER: &str = "# nipart: dynamic";
const FOREIGN_MARKER: &str = "# nipart: foreign";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum EntryOrigin {
    Static,
    Dynamic,
    #[default]
    Foreign,
}

#[derive(Debug, Default)]
struct ResolvConf {
    /// `(origin, server)` in file order.
    servers: Vec<(EntryOrigin, String)>,
    /// `(origin, search_domain)` in file order.  The keyword kind
    /// (`search` or `domain`) is normalized to a search entry.
    searches: Vec<(EntryOrigin, String)>,
    /// `(origin, option)` in file order.
    options: Vec<(EntryOrigin, String)>,
}

impl ResolvConf {
    fn parse(content: &str) -> Self {
        let mut ret = Self::default();
        let mut origin = EntryOrigin::Foreign;
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with('#') {
                match trimmed {
                    STATIC_MARKER => origin = EntryOrigin::Static,
                    DYNAMIC_MARKER => origin = EntryOrigin::Dynamic,
                    FOREIGN_MARKER => origin = EntryOrigin::Foreign,
                    _ => (),
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
                        ret.servers.push((origin, srv.to_string()));
                    }
                }
                "search" | "domain" => {
                    for domain in parts {
                        ret.searches.push((origin, domain.to_string()));
                    }
                }
                "options" => {
                    for opt in parts {
                        ret.options.push((origin, opt.to_string()));
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

    fn entries_of(
        &self,
        entries: &[(EntryOrigin, String)],
        origin: EntryOrigin,
    ) -> Vec<String> {
        entries
            .iter()
            .filter(|(entry_origin, _)| *entry_origin == origin)
            .map(|(_, value)| value.clone())
            .collect()
    }

    fn foreign_servers(&self) -> Vec<String> {
        self.entries_of(&self.servers, EntryOrigin::Foreign)
    }

    fn foreign_searches(&self) -> Vec<String> {
        self.entries_of(&self.searches, EntryOrigin::Foreign)
    }

    fn foreign_options(&self) -> Vec<String> {
        self.entries_of(&self.options, EntryOrigin::Foreign)
    }

    #[cfg(test)]
    fn static_servers(&self) -> Vec<String> {
        self.entries_of(&self.servers, EntryOrigin::Static)
    }

    #[cfg(test)]
    fn static_searches(&self) -> Vec<String> {
        self.entries_of(&self.searches, EntryOrigin::Static)
    }

    #[cfg(test)]
    fn static_options(&self) -> Vec<String> {
        self.entries_of(&self.options, EntryOrigin::Static)
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

/// Write the DNS configuration to `/etc/resolv.conf`.
///
/// Nipart-managed entries are written with explicit origin markers so a
/// later read can tell static entries, DHCP-learned dynamic entries and
/// entries owned by other tools apart. Static entries always come first,
/// followed by dynamic entries and finally preserved foreign entries.
///
/// `cache_bind_ip` is the DNS cache listen address which must be written
/// first (when the cache is enabled). When the cache is enabled, dynamic
/// and foreign nameservers are not written: the cache handles the dynamic
/// upstreams itself and the host resolver must only query the cache.
pub(crate) fn write_static_dns(
    cache_bind_ip: Option<IpAddr>,
    static_servers: &[String],
    static_searches: &[String],
    static_options: &[String],
    dynamic_servers: &[String],
    dynamic_searches: &[String],
) -> Result<(), NipartError> {
    let existing = ResolvConf::read()?;
    let cache_enabled = cache_bind_ip.is_some();
    let mut static_srvs: Vec<String> = Vec::new();
    if let Some(bind_ip) = cache_bind_ip {
        // The cache must be the first nameserver so the host resolver
        // always queries it first (the same ordering is enforced by
        // `DnsResolver::validate()`).
        push_unique(&mut static_srvs, bind_ip.to_string());
    }
    for srv in static_servers {
        push_unique(&mut static_srvs, srv.clone());
    }
    let mut static_search_list = static_searches.to_vec();
    dedup(&mut static_search_list);
    let mut static_option_list = static_options.to_vec();
    dedup(&mut static_option_list);

    let mut auto_servers: Vec<String> = Vec::new();
    let mut auto_searches: Vec<String> = Vec::new();
    if !cache_enabled {
        for srv in dynamic_servers {
            push_unique(&mut auto_servers, srv.clone());
        }
        for domain in dynamic_searches {
            if !static_search_list.contains(domain) {
                push_unique(&mut auto_searches, domain.clone());
            }
        }
    }

    let mut foreign_servers: Vec<String> = Vec::new();
    if !cache_enabled {
        for srv in existing.foreign_servers() {
            if !static_srvs.contains(&srv) {
                push_unique(&mut foreign_servers, srv);
            }
        }
    }
    let mut foreign_searches: Vec<String> = Vec::new();
    for domain in existing.foreign_searches() {
        if !static_search_list.contains(&domain)
            && !auto_searches.contains(&domain)
        {
            push_unique(&mut foreign_searches, domain);
        }
    }
    let mut foreign_options: Vec<String> = Vec::new();
    for option in existing.foreign_options() {
        if !static_option_list.contains(&option) {
            push_unique(&mut foreign_options, option);
        }
    }

    write_resolv_conf(
        &static_srvs,
        &static_search_list,
        &static_option_list,
        &auto_servers,
        &auto_searches,
        &foreign_servers,
        &foreign_searches,
        &foreign_options,
    )
}

/// Remove nipart managed DNS configuration from `/etc/resolv.conf`,
/// preserving entries owned by other tools.
pub(crate) fn purge_dns_resolver() -> Result<(), NipartError> {
    let existing = ResolvConf::read()?;
    write_resolv_conf(
        &[],
        &[],
        &[],
        &[],
        &[],
        &existing.foreign_servers(),
        &existing.foreign_searches(),
        &existing.foreign_options(),
    )
}

#[allow(clippy::too_many_arguments)]
fn write_resolv_conf(
    static_servers: &[String],
    static_searches: &[String],
    static_options: &[String],
    auto_servers: &[String],
    auto_searches: &[String],
    foreign_servers: &[String],
    foreign_searches: &[String],
    foreign_options: &[String],
) -> Result<(), NipartError> {
    let mut content = String::new();
    write_section(
        &mut content,
        STATIC_MARKER,
        static_servers,
        static_searches,
        static_options,
    );
    write_section(
        &mut content,
        DYNAMIC_MARKER,
        auto_servers,
        auto_searches,
        &[],
    );
    write_section(
        &mut content,
        FOREIGN_MARKER,
        foreign_servers,
        foreign_searches,
        foreign_options,
    );

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

fn write_section(
    content: &mut String,
    marker: &str,
    servers: &[String],
    searches: &[String],
    options: &[String],
) {
    if servers.is_empty() && searches.is_empty() && options.is_empty() {
        return;
    }
    content.push_str(marker);
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
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn dedup(values: &mut Vec<String>) {
    let mut seen: Vec<String> = Vec::new();
    values.retain(|value| {
        if seen.contains(value) {
            false
        } else {
            seen.push(value.clone());
            true
        }
    });
}

impl NipartNoDaemon {
    /// Query the running DNS resolver state from `/etc/resolv.conf`.
    pub async fn query_dns_resolver() -> Result<DnsResolver, NipartError> {
        query_running_dns()
    }

    /// Apply the DNS resolver configuration to `/etc/resolv.conf` from
    /// explicit static and dynamic server/search/option lists.
    #[allow(clippy::too_many_arguments)]
    pub async fn apply_dns_resolver_conf(
        cache_bind_ip: Option<IpAddr>,
        static_servers: &[String],
        static_searches: &[String],
        static_options: &[String],
        dynamic_servers: &[String],
        dynamic_searches: &[String],
    ) -> Result<(), NipartError> {
        write_static_dns(
            cache_bind_ip,
            static_servers,
            static_searches,
            static_options,
            dynamic_servers,
            dynamic_searches,
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
        assert_eq!(conf.foreign_servers(), vec!["192.0.2.1".to_string()]);
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
    fn test_parse_resolv_conf_origins() {
        let content = "\
# nipart: static
nameserver 192.0.2.53
search example.org
# nipart: dynamic
nameserver 198.51.100.53
search example.net
# nipart: foreign
nameserver 203.0.113.53
";
        let conf = ResolvConf::parse(content);
        assert_eq!(conf.static_servers(), vec!["192.0.2.53".to_string()]);
        assert_eq!(conf.static_searches(), vec!["example.org".to_string()]);
        assert_eq!(
            conf.entries_of(&conf.servers, EntryOrigin::Dynamic),
            vec!["198.51.100.53".to_string()]
        );
        assert_eq!(
            conf.entries_of(&conf.searches, EntryOrigin::Dynamic),
            vec!["example.net".to_string()]
        );
        assert_eq!(conf.foreign_servers(), vec!["203.0.113.53".to_string()]);
    }

    #[test]
    fn test_write_section_skips_empty() {
        let mut content = String::new();
        write_section(&mut content, "# test", &[], &[], &[]);
        assert!(content.is_empty());
        write_section(
            &mut content,
            "# test",
            &["192.0.2.53".to_string()],
            &["example.org".to_string()],
            &[],
        );
        assert_eq!(
            content,
            "# test\nnameserver 192.0.2.53\nsearch example.org\n"
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
        let err = NipartNoDaemon::apply_dns_resolver(&merged_state, &[])
            .await
            .expect_err("no-daemon mode must reject the DNS cache");
        assert_eq!(err.kind(), crate::ErrorKind::NoSupport);
    }
}
