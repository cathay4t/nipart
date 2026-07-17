// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nmstate project(https://nmstate.io/) which
// is under license of Apache 2.0, author of original file is:
//  * Gris Ge <fge@redhat.com>

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use serde::{Deserialize, Serialize};

use crate::{
    BaseInterface, ErrorKind, InterfaceIpAddr, InterfaceIpv4, InterfaceIpv6,
    InterfaceState, InterfaceType, JsonDisplay, NipartError, NipartInterface,
};

/// Loopback Interface
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonDisplay)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[non_exhaustive]
pub struct LoopbackInterface {
    #[serde(flatten)]
    pub base: BaseInterface,
}

impl LoopbackInterface {
    pub fn new(base: BaseInterface) -> Self {
        Self {
            base,
            ..Default::default()
        }
    }
}

impl Default for LoopbackInterface {
    fn default() -> Self {
        Self {
            base: BaseInterface {
                name: "lo".into(),
                kernel_iface_name: "lo".into(),
                iface_type: InterfaceType::Loopback,
                state: InterfaceState::Up,
                mtu: Some(65536),
                ipv4: Some(InterfaceIpv4 {
                    enabled: Some(true),
                    dhcp: Some(false),
                    addresses: Some(vec![InterfaceIpAddr {
                        ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
                        prefix_length: 8,
                        ..Default::default()
                    }]),
                    ..Default::default()
                }),
                ipv6: Some(InterfaceIpv6 {
                    enabled: Some(true),
                    autoconf: Some(false),
                    dhcp: Some(false),
                    addresses: Some(vec![InterfaceIpAddr {
                        ip: IpAddr::V6(Ipv6Addr::LOCALHOST),
                        prefix_length: 128,
                        ..Default::default()
                    }]),
                }),
                ..Default::default()
            },
        }
    }
}

impl NipartInterface for LoopbackInterface {
    fn base_iface(&self) -> &BaseInterface {
        &self.base
    }

    fn base_iface_mut(&mut self) -> &mut BaseInterface {
        &mut self.base
    }

    // Loopback interface is virtual interface, but we should never allow
    // deletion on this interface, hence return false here.
    fn is_virtual(&self) -> bool {
        false
    }

    /// * Loopback interface should always have 127.0.0.1 and ::1 IP address
    ///   regardless what user desired.
    /// * Absent of loopback interface means revert to default.
    fn sanitize(
        &self,
        _current: Option<&Self>,
        for_save: &mut Self,
        for_apply: &mut Self,
        for_verify: &mut Self,
        merged: &mut Self,
    ) -> Result<(), NipartError> {
        let desired = self;
        if desired.is_absent() {
            log::info!(
                "Marking loopback interface as absent means revert loopback \
                 interface to default state"
            );
            *for_save = desired.clone();
            *for_apply = Self::default();
            *merged = Self::default();
            return Ok(());
        }
        let default_ipv4_addr = InterfaceIpAddr {
            ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            prefix_length: 8,
            ..Default::default()
        };
        let default_ipv6_addr = InterfaceIpAddr {
            ip: IpAddr::V6(Ipv6Addr::LOCALHOST),
            prefix_length: 128,
            ..Default::default()
        };
        if let Some(ipv4) = for_apply.base.ipv4.as_mut() {
            if !ipv4.is_enabled() {
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    "Disabling IPv4 on loopback interface is not allowed"
                        .to_string(),
                ));
            }
            if let Some(addrs) = ipv4.addresses.as_mut()
                && !addrs.contains(&default_ipv4_addr)
            {
                log::info!(
                    "Appending 127.0.0.1/8 address to desired IPv4 addresses \
                     of loopback"
                );
                addrs.push(default_ipv4_addr);
            }
            for_save.base.ipv4 = Some(ipv4.clone());
            for_verify.base.ipv4 = Some(ipv4.clone());
            merged.base.ipv4 = Some(ipv4.clone());
        }

        // TODO: user might disable IPv6 globally.
        if let Some(ipv6) = for_apply.base.ipv6.as_mut() {
            if !ipv6.is_enabled() {
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    "Disabling IPv6 on loopback interface is not allowed"
                        .to_string(),
                ));
            }
            if let Some(addrs) = ipv6.addresses.as_mut()
                && !addrs.contains(&default_ipv6_addr)
            {
                log::info!(
                    "Appending ::1/128 address to desired IPv6 addresses of \
                     loopback"
                );
                addrs.push(default_ipv6_addr);
            }
            for_verify.base.ipv6 = Some(ipv6.clone());
            for_save.base.ipv6 = Some(ipv6.clone());
            merged.base.ipv6 = Some(ipv6.clone());
        }
        Ok(())
    }
}
