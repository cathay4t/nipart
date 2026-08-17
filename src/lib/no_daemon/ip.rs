// SPDX-License-Identifier: Apache-2.0

use std::{net::IpAddr, str::FromStr};

use super::iface::init_np_iface;
use crate::{
    BaseInterface, InterfaceIpAddr, InterfaceIpv4, InterfaceIpv6, NipartError,
    NipartNoDaemon,
};

pub(crate) fn np_ipv4_to_nipart(
    np_iface: &nispor::Iface,
) -> Option<InterfaceIpv4> {
    if let Some(np_ip) = &np_iface.ipv4 {
        let mut ip = InterfaceIpv4 {
            enabled: Some(!np_ip.addresses.is_empty()),
            ..Default::default()
        };
        if !ip.is_enabled() {
            return Some(ip);
        }
        let mut addresses = Vec::new();
        for np_addr in &np_ip.addresses {
            if np_addr.valid_lft != "forever" {
                ip.dhcp = Some(true);
            }
            match std::net::IpAddr::from_str(np_addr.address.as_str()) {
                Ok(i) => {
                    let addr = InterfaceIpAddr {
                        ip: i,
                        prefix_length: np_addr.prefix_len,
                        valid_life_time: if np_addr.valid_lft != "forever" {
                            Some(np_addr.valid_lft.clone())
                        } else {
                            None
                        },
                        preferred_life_time: if np_addr.preferred_lft
                            != "forever"
                        {
                            Some(np_addr.preferred_lft.clone())
                        } else {
                            None
                        },
                        ..Default::default()
                    };
                    addresses.push(addr);
                }
                Err(e) => {
                    log::warn!(
                        "BUG: nispor got invalid IP address {}, error {}",
                        np_addr.address.as_str(),
                        e
                    );
                }
            }
        }
        ip.addresses = Some(addresses);
        if ip.dhcp.is_none() {
            ip.dhcp = Some(false);
        }
        Some(ip)
    } else {
        // IP might just disabled
        Some(InterfaceIpv4::default())
    }
}

pub(crate) fn np_ipv6_to_nipart(
    np_iface: &nispor::Iface,
) -> Option<InterfaceIpv6> {
    if let Some(np_ip) = &np_iface.ipv6 {
        let mut ip = InterfaceIpv6 {
            enabled: Some(!np_ip.addresses.is_empty()),
            ..Default::default()
        };

        if !ip.is_enabled() {
            return Some(ip);
        }
        let mut addresses = Vec::new();
        for np_addr in &np_ip.addresses {
            // A `kernel_ra` address might hold `forever` lifetime (e.g. RA
            // with infinite valid lifetime), detect it via the address
            // protocol so autoconf can still be verified.
            if np_addr.protocol
                == Some(nispor::AddressProtocol::RouterAnnouncement)
            {
                ip.autoconf = Some(true);
            } else if np_addr.valid_lft != "forever" {
                if np_addr.prefix_len == 128 {
                    ip.dhcp = Some(true);
                } else {
                    ip.autoconf = Some(true);
                }
            }
            match std::net::IpAddr::from_str(np_addr.address.as_str()) {
                Ok(i) => {
                    let addr = InterfaceIpAddr {
                        ip: i,
                        prefix_length: np_addr.prefix_len,
                        valid_life_time: if np_addr.valid_lft != "forever" {
                            Some(np_addr.valid_lft.clone())
                        } else {
                            None
                        },
                        preferred_life_time: if np_addr.preferred_lft
                            != "forever"
                        {
                            Some(np_addr.preferred_lft.clone())
                        } else {
                            None
                        },
                        ..Default::default()
                    };
                    addresses.push(addr);
                }
                Err(e) => {
                    log::warn!(
                        "BUG: nispor got invalid IP address {}, error {}",
                        np_addr.address.as_str(),
                        e
                    );
                }
            }
        }
        ip.addresses = Some(addresses);
        if ip.autoconf.is_none() && ip.dhcp.is_none() {
            ip.dhcp = Some(false);
            ip.autoconf = Some(false);
        }
        Some(ip)
    } else {
        // IP might just disabled
        Some(InterfaceIpv6::default())
    }
}

pub(crate) fn apply_iface_ip_changes(
    des_iface: &BaseInterface,
    cur_iface: Option<&BaseInterface>,
) -> Result<Option<nispor::IfaceConf>, NipartError> {
    if des_iface.is_absent() {
        return Ok(None);
    }

    let mut np_iface = init_np_iface(des_iface);

    let init_np_iface = np_iface.clone();

    let empty_iface = des_iface.clone_name_type_only();

    let cur_iface = cur_iface.unwrap_or(&empty_iface);

    if des_iface.ipv4.as_ref() != cur_iface.ipv4.as_ref()
        && let Some(des_ipv4) = des_iface.ipv4.as_ref()
    {
        let mut des_addrs: &[InterfaceIpAddr] = &[];
        if des_ipv4.is_enabled()
            && let Some(d) = des_ipv4.addresses.as_ref()
        {
            des_addrs = d;
        }

        let mut cur_addrs: &[InterfaceIpAddr] = &[];
        if let Some(cur_ipv4) = cur_iface.ipv4.as_ref()
            && cur_ipv4.is_enabled()
            && let Some(c) = cur_ipv4.addresses.as_ref()
        {
            cur_addrs = c;
        }
        let np_addrs = nipart_ip_addrs_to_nispor(des_addrs, cur_addrs);

        if !np_addrs.is_empty() {
            let mut np_ip_conf = nispor::IpConf::default();
            np_ip_conf.addresses = np_addrs;
            np_iface.ipv4 = Some(np_ip_conf);
        }
    }

    if des_iface.ipv6.as_ref() != cur_iface.ipv6.as_ref()
        && let Some(des_ipv6) = des_iface.ipv6.as_ref()
    {
        let mut des_addrs: &[InterfaceIpAddr] = &[];
        if des_ipv6.is_enabled()
            && let Some(d) = des_ipv6.addresses.as_ref()
        {
            des_addrs = d;
        }

        // The IPv6 link-local address is auto-assigned by kernel, it should
        // not be removed by the apply action while IPv6 stays enabled,
        // otherwise the DHCPv6 client would lose the source address for its
        // traffic. When IPv6 is explicitly disabled, all addresses including
        // the link-local one are purged.
        let cur_addrs: Vec<InterfaceIpAddr> = {
            let mut addrs = Vec::new();
            if let Some(cur_ipv6) = cur_iface.ipv6.as_ref()
                && cur_ipv6.is_enabled()
                && let Some(c) = cur_ipv6.addresses.as_ref()
            {
                for cur_addr in c.iter().filter(|a| {
                    if des_ipv6.is_enabled() {
                        match a.ip {
                            IpAddr::V6(ip_addr) => {
                                !ip_addr.is_unicast_link_local()
                            }
                            IpAddr::V4(_) => true,
                        }
                    } else {
                        true
                    }
                }) {
                    addrs.push(cur_addr.clone());
                }
            }
            addrs
        };
        let np_addrs = nipart_ip_addrs_to_nispor(des_addrs, &cur_addrs);

        if !np_addrs.is_empty() {
            let mut np_ip_conf = nispor::IpConf::default();
            np_ip_conf.addresses = np_addrs;
            np_iface.ipv6 = Some(np_ip_conf);
        }
    }

    if np_iface != init_np_iface {
        Ok(Some(np_iface))
    } else {
        Ok(None)
    }
}

fn nipart_ip_addr_to_nispor(
    ip_addr: &InterfaceIpAddr,
    remove: bool,
) -> nispor::IpAddrConf {
    let mut np_ip_addr = nispor::IpAddrConf::default();
    np_ip_addr.address = ip_addr.ip.to_string();
    np_ip_addr.prefix_len = ip_addr.prefix_length;
    np_ip_addr.preferred_lft = ip_addr
        .preferred_life_time
        .clone()
        .unwrap_or("forever".to_string());
    np_ip_addr.valid_lft = ip_addr
        .valid_life_time
        .clone()
        .unwrap_or("forever".to_string());
    np_ip_addr.remove = remove;

    np_ip_addr
}

fn nipart_ip_addrs_to_nispor(
    des_addrs: &[InterfaceIpAddr],
    cur_addrs: &[InterfaceIpAddr],
) -> Vec<nispor::IpAddrConf> {
    let mut ret: Vec<nispor::IpAddrConf> = Vec::new();

    if is_appending(des_addrs, cur_addrs) {
        for cur_addr in cur_addrs {
            if !des_addrs.contains(cur_addr) {
                ret.push(nipart_ip_addr_to_nispor(cur_addr, true));
            }
        }
        for des_addr in des_addrs {
            if !cur_addrs.contains(des_addr) {
                ret.push(nipart_ip_addr_to_nispor(des_addr, false));
            }
        }
    } else if is_replacing(des_addrs, cur_addrs) {
        for des_addr in des_addrs {
            ret.push(nipart_ip_addr_to_nispor(des_addr, false));
        }
    } else {
        // Purge all current IP address, so we get expected IP address order.
        for cur_addr in cur_addrs {
            ret.push(nipart_ip_addr_to_nispor(cur_addr, true));
        }
        for des_addr in des_addrs {
            ret.push(nipart_ip_addr_to_nispor(des_addr, false));
        }
    }

    ret
}

fn is_appending(
    des_addrs: &[InterfaceIpAddr],
    cur_addrs: &[InterfaceIpAddr],
) -> bool {
    cur_addrs.len() < des_addrs.len()
        && &des_addrs[..cur_addrs.len()] == cur_addrs
}

fn is_replacing(
    des_addrs: &[InterfaceIpAddr],
    cur_addrs: &[InterfaceIpAddr],
) -> bool {
    cur_addrs.len() == des_addrs.len()
        && des_addrs.iter().all(|des_addr| {
            cur_addrs.iter().any(|cur_addr| {
                des_addr.ip == cur_addr.ip
                    && des_addr.prefix_length == cur_addr.prefix_length
            })
        })
}

impl NipartNoDaemon {
    /// Purge all IPv4 and IPv6 addresses from the interface.
    ///
    /// Used before restarting DHCP when the wifi-phy switches to a
    /// different SSID, so the previous network's lease cannot survive
    /// the switch.
    pub async fn purge_iface_ip(
        base_iface: &BaseInterface,
        current: Option<&BaseInterface>,
    ) -> Result<(), NipartError> {
        let mut purge_iface = base_iface.clone_name_type_only();
        purge_iface.ipv4 = Some(InterfaceIpv4::new_disabled());
        purge_iface.ipv6 = Some(InterfaceIpv6::new_disabled());
        if let Some(np_iface) = apply_iface_ip_changes(&purge_iface, current)? {
            let mut net_conf = nispor::NetConf::default();
            net_conf.ifaces = Some(vec![np_iface]);
            net_conf.apply_async().await.map_err(|e| {
                NipartError::new(
                    crate::ErrorKind::Bug,
                    format!("Failed to purge IP addresses: {e}"),
                )
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv6Addr};

    use super::apply_iface_ip_changes;
    use crate::{BaseInterface, InterfaceIpAddr, InterfaceIpv6, InterfaceType};

    fn iface_with_ipv6(ipv6: InterfaceIpv6) -> BaseInterface {
        let mut iface =
            BaseInterface::new("eth0".to_string(), InterfaceType::Ethernet);
        iface.ipv6 = Some(ipv6);
        iface
    }

    fn ipv6_addr(ip: &str, prefix_length: u8) -> InterfaceIpAddr {
        InterfaceIpAddr {
            ip: IpAddr::V6(ip.parse::<Ipv6Addr>().unwrap()),
            prefix_length,
            valid_life_time: None,
            preferred_life_time: None,
        }
    }

    /// The kernel auto-assigned IPv6 link-local address must not be removed
    /// while IPv6 stays enabled, otherwise the DHCPv6 client would lose the
    /// source address for its traffic.
    #[test]
    fn test_link_local_not_removed_when_ipv6_enabled() {
        let des_iface = iface_with_ipv6(InterfaceIpv6 {
            enabled: Some(true),
            dhcp: Some(true),
            addresses: None,
            ..Default::default()
        });
        let cur_iface = iface_with_ipv6(InterfaceIpv6 {
            enabled: Some(true),
            addresses: Some(vec![
                ipv6_addr("fe80::1", 64),
                ipv6_addr("2001:db8::1", 64),
            ]),
            ..Default::default()
        });

        let np_iface =
            apply_iface_ip_changes(&des_iface, Some(&cur_iface)).unwrap();
        let np_addrs = np_iface.unwrap().ipv6.unwrap().addresses;

        // The static global address is purged when switching to DHCP...
        assert!(
            np_addrs
                .iter()
                .any(|a| { a.remove && a.address == "2001:db8::1" })
        );
        // ...but the link-local address is never removed.
        assert!(
            np_addrs
                .iter()
                .all(|a| { !(a.remove && a.address.starts_with("fe80")) })
        );
    }

    /// When IPv6 is explicitly disabled, the link-local address is purged
    /// together with all other IPv6 addresses.
    #[test]
    fn test_link_local_removed_when_ipv6_disabled() {
        let des_iface = iface_with_ipv6(InterfaceIpv6 {
            enabled: Some(false),
            ..Default::default()
        });
        let cur_iface = iface_with_ipv6(InterfaceIpv6 {
            enabled: Some(true),
            addresses: Some(vec![ipv6_addr("fe80::1", 64)]),
            ..Default::default()
        });

        let np_iface =
            apply_iface_ip_changes(&des_iface, Some(&cur_iface)).unwrap();
        let np_addrs = np_iface.unwrap().ipv6.unwrap().addresses;

        assert!(
            np_addrs
                .iter()
                .any(|a| { a.remove && a.address == "fe80::1" })
        );
    }
}
