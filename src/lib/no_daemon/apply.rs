// SPDX-License-Identifier: Apache-2.0

use super::{
    dhcp::DhcpDns, inter_ifaces::apply_ifaces, route::apply_routes,
    route_rule::apply_route_rules,
};
use crate::{
    ErrorKind, InterfaceType, MergedNetworkState, NetworkState,
    NipartApplyOption, NipartError, NipartInterface, NipartNoDaemon,
};

const RETRY_COUNT_COMMON: usize = 10;
const RETRY_COUNT_WIFI: usize = 20;
const RETRY_INTERVAL_MS: u64 = 500;
// Top-level apply retry (no-daemon mode): mirror the daemon retry so a
// transient failure of the apply itself (e.g. a NIC not ready yet) is
// retried once after this interval before the error is reported.
const TOP_LEVEL_RETRY_MAX: u32 = 2;
const TOP_LEVEL_RETRY_INTERVAL_SEC: u64 = 2;

impl NipartNoDaemon {
    /// Apply the desired state in no-daemon mode.
    ///
    /// The whole apply retries once after [TOP_LEVEL_RETRY_INTERVAL_SEC] when
    /// it fails with a retriable error; non-retriable errors
    /// (`ErrorKind::retriable()`) are reported immediately.
    pub async fn apply_network_state(
        desired_state: NetworkState,
        option: NipartApplyOption,
    ) -> Result<NetworkState, NipartError> {
        let mut last_err: Option<NipartError> = None;
        for attempt in 1..=TOP_LEVEL_RETRY_MAX {
            match Self::apply_network_state_once(
                desired_state.clone(),
                option.clone(),
            )
            .await
            {
                Ok(state) => return Ok(state),
                Err(e) => {
                    if !e.kind().retriable() {
                        return Err(e);
                    }
                    if attempt < TOP_LEVEL_RETRY_MAX {
                        log::warn!(
                            "Apply failed (attempt \
                             {attempt}/{TOP_LEVEL_RETRY_MAX}), retrying in \
                             {TOP_LEVEL_RETRY_INTERVAL_SEC} seconds: {e}"
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(
                            TOP_LEVEL_RETRY_INTERVAL_SEC,
                        ))
                        .await;
                    } else {
                        log::warn!(
                            "Apply failed (attempt \
                             {attempt}/{TOP_LEVEL_RETRY_MAX}): {e}"
                        );
                    }
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| {
            NipartError::new(
                ErrorKind::Bug,
                "BUG: apply retry loop finished without an error".to_string(),
            )
        }))
    }

    async fn apply_network_state_once(
        desired_state: NetworkState,
        option: NipartApplyOption,
    ) -> Result<NetworkState, NipartError> {
        let current_state =
            Self::query_network_state(Default::default()).await?;

        log::trace!("Applying {desired_state} with option {option}");
        let mut merged_state = MergedNetworkState::new(
            desired_state.clone(),
            current_state.clone(),
            None,
            option.clone(),
        )?;

        for iface in
            merged_state.ifaces.iter().filter(|i| i.for_apply.is_some())
        {
            if let Some(cur_iface) = iface.current.as_ref() {
                log::trace!("Current interface {cur_iface}");
            }
            if let Some(apply_iface) = iface.for_apply.as_ref() {
                log::trace!("Applying interface changes: {apply_iface}");
            }
        }

        // TODO(Gris Ge): Special sanitize for NoDaemon mode:
        //  * DHCP not supported
        //  * controller and IP setting for `wifi-cfg` interface

        Self::apply_merged_state(&mut merged_state).await?;
        let mut dhcp_dns = Vec::new();
        if option.dhcp_in_no_daemon {
            dhcp_dns = Self::run_dhcp_once(&merged_state.ifaces).await?;
        }
        Self::apply_dns_resolver(&merged_state, &dhcp_dns).await?;

        let max_retry_count = get_max_retry_count(&merged_state);

        let mut result: Result<(), NipartError> = Ok(());
        if !option.no_verify {
            for cur_retry_count in 1..(max_retry_count + 1) {
                let post_apply_current_state =
                    Self::query_network_state(Default::default()).await?;
                log::trace!(
                    "Post apply network state: {post_apply_current_state}"
                );
                if cur_retry_count == max_retry_count / 2 {
                    log::info!("Apply the desired state again");
                    Self::apply_merged_state(&mut merged_state).await?;
                    if option.dhcp_in_no_daemon {
                        dhcp_dns =
                            Self::run_dhcp_once(&merged_state.ifaces).await?;
                    }
                    Self::apply_dns_resolver(&merged_state, &dhcp_dns).await?;
                }
                result = merged_state.verify(&post_apply_current_state);
                if let Err(e) = &result {
                    log::info!(
                        "Retrying({cur_retry_count}/{max_retry_count}) on \
                         verification error: {e}"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(
                        RETRY_INTERVAL_MS,
                    ))
                    .await;
                } else {
                    break;
                }
            }
        }
        result?;

        let diff_state = merged_state.gen_diff()?;

        Ok(diff_state)
    }

    pub async fn apply_merged_state(
        merged_state: &mut MergedNetworkState,
    ) -> Result<(), NipartError> {
        apply_ifaces(&mut merged_state.ifaces).await?;
        apply_routes(&merged_state.routes).await?;
        apply_route_rules(&merged_state.route_rules).await?;
        Ok(())
    }

    /// Apply the standard DNS resolver configuration in no-daemon mode.
    ///
    /// The DNS cache is a daemon task: in no-daemon mode an enabled cache
    /// is rejected instead of silently left running without supervision.
    pub(crate) async fn apply_dns_resolver(
        merged_state: &MergedNetworkState,
        dhcp_dns: &[DhcpDns],
    ) -> Result<(), NipartError> {
        if let Some(cache) = merged_state.dns.cache()
            && cache.enabled
        {
            return Err(NipartError::new(
                ErrorKind::NoSupport,
                "DNS cache requires the nipart daemon; it is not supported in \
                 no-daemon mode"
                    .to_string(),
            ));
        }
        let cache_bind_ip = merged_state
            .dns
            .cache()
            .filter(|cache| cache.enabled)
            .and_then(|cache| cache.bind_addr())
            .map(|addr| addr.ip());
        let (dynamic_servers, dynamic_domains) =
            gen_dhcp_dns(merged_state, dhcp_dns);
        let dynamic_searches = if merged_state.dns.auto_search_allowed() {
            dynamic_domains
        } else {
            Vec::new()
        };
        Self::apply_dns_resolver_conf(
            cache_bind_ip,
            &merged_state.dns.servers,
            &merged_state.dns.searches,
            &merged_state.dns.options,
            &dynamic_servers,
            &dynamic_searches,
        )
        .await
    }
}

/// Effective DHCP DNS servers and search domains for no-daemon mode.
fn gen_dhcp_dns(
    merged_state: &MergedNetworkState,
    dhcp_dns: &[DhcpDns],
) -> (Vec<String>, Vec<String>) {
    struct Entry<'a> {
        priority: u32,
        order: usize,
        iface: &'a str,
        dns: &'a DhcpDns,
    }
    let mut entries: Vec<Entry> = Vec::new();
    for (order, merged_iface) in merged_state.ifaces.iter().enumerate() {
        let base = merged_iface.merged.base_iface();
        let Some(dns) = dhcp_dns.iter().find(|dns| {
            dns.iface == base.kernel_iface_name || dns.iface == base.name
        }) else {
            continue;
        };
        let Some(ipv4) = base.ipv4.as_ref() else {
            continue;
        };
        if !ipv4.auto_dns_enabled() {
            continue;
        }
        entries.push(Entry {
            priority: ipv4.auto_dns_priority_value(),
            order,
            iface: base.kernel_iface_name.as_str(),
            dns,
        });
    }
    entries.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then(a.order.cmp(&b.order))
            .then(a.iface.cmp(b.iface))
    });
    let mut servers: Vec<String> = Vec::new();
    let mut domains: Vec<String> = Vec::new();
    for entry in entries {
        for srv in &entry.dns.nameservers {
            if !servers.contains(srv) {
                servers.push(srv.clone());
            }
        }
        for domain in &entry.dns.domains {
            let domain = domain.to_lowercase();
            if !domains.contains(&domain) {
                domains.push(domain);
            }
        }
    }
    (servers, domains)
}

fn get_max_retry_count(merged_state: &MergedNetworkState) -> usize {
    if merged_state
        .ifaces
        .kernel_ifaces
        .values()
        .any(|merged_iface| {
            merged_iface.for_apply.is_some()
                && matches!(
                    merged_iface.merged.iface_type(),
                    InterfaceType::WifiPhy | InterfaceType::WifiCfg
                )
        })
    {
        RETRY_COUNT_WIFI
    } else {
        RETRY_COUNT_COMMON
    }
}
