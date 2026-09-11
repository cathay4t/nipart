// SPDX-License-Identifier: Apache-2.0

//! Resolve the `auto-dns` upstream nameservers for the DNS cache.
//!
//! Nipart has no connection manager, so the dynamic nameservers are
//! collected from the running DHCPv4/DHCPv6 workers.  IPv6-RA learned
//! nameservers are not supported yet.

use std::net::IpAddr;

use nipart::NipartError;

use super::commander::NipartCommander;

impl NipartCommander {
    /// Collect nameservers currently learned from DHCP.
    pub(crate) async fn auto_dns_servers(
        &mut self,
    ) -> Result<Vec<IpAddr>, NipartError> {
        let mut ret: Vec<IpAddr> = Vec::new();
        let v4_nameservers = self.dhcpv4_manager.nameservers().await?;
        let mut iface_names: Vec<&String> = v4_nameservers.keys().collect();
        iface_names.sort();
        for iface_name in iface_names {
            for srv in &v4_nameservers[iface_name] {
                push_unique_ip(&mut ret, srv);
            }
        }
        // TODO: DHCPv6 learned nameservers.  mozim 0.3.2 does not expose
        // the DHCPv6 DNS Recursive Name Server option yet.
        Ok(ret)
    }

    /// Refresh the DNS cache upstreams after the DHCP lease changed.
    ///
    /// Only a running cache is touched; a stopped cache has no upstream
    /// to update and will pick up the latest nameservers on its next
    /// apply.
    pub(crate) async fn refresh_dns_cache_auto_dns(
        &mut self,
    ) -> Result<(), NipartError> {
        let servers = self.auto_dns_servers().await?;
        self.dns_manager.refresh_auto_dns(&servers).await
    }
}

fn push_unique_ip(nameservers: &mut Vec<IpAddr>, srv: &str) {
    // DHCPv6 nameservers may carry a scope id.
    let srv = srv.split('%').next().unwrap_or(srv);
    if let Ok(ip) = srv.parse::<IpAddr>()
        && !nameservers.contains(&ip)
    {
        nameservers.push(ip);
    }
}
