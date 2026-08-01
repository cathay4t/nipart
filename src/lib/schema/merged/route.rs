// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nmstate project(https://nmstate.io/) which
// is under license of Apache 2.0, authors of original file are:
//  * Gris Ge <fge@redhat.com>
//  * Wen Liang <liangwen12year@gmail.com>
//  * Jan Vaclav <jvaclav@redhat.com>
//  * Íñigo Huguet <ihuguet@redhat.com>
//  * Fernando Fernandez Mancera <ffmancera@riseup.net>

use std::collections::{HashMap, HashSet, hash_map::Entry};

use serde::{Deserialize, Serialize};

use crate::{
    ErrorKind, JsonDisplay, MergedInterfaces, NipartError, NipartInterface,
    RouteEntry, RouteState, Routes,
};

const LOOPBACK_IFACE_NAME: &str = "lo";

struct IfaceLists<'a> {
    absent: HashSet<&'a str>,
    ipv4_disabled: HashSet<&'a str>,
    ipv6_disabled: HashSet<&'a str>,
    dhcpv4_enabled: HashSet<&'a str>,
    will_delete: HashSet<&'a str>,
}

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonDisplay,
)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct MergedRoutes {
    // When all routes next hop to a interface are all marked as absent,
    // the `MergedRoutes.merged` will not have entry for this interface, but
    // interface name is found in `MergedRoutes.route_changed_ifaces`.
    // For backend use incremental route changes, please use
    // `MergedRoutes.changed_routes`.
    pub merged: HashMap<String, Vec<RouteEntry>>,
    pub route_changed_ifaces: Vec<String>,
    // The `changed_routes` contains desired new routes and also including
    // current routes been marked as absent. Not including desired route equal
    // to current route.
    pub changed_routes: Vec<RouteEntry>,
    pub desired: Routes,
    pub current: Routes,
    #[serde(default)]
    pub saved: Option<Routes>,
}

impl MergedRoutes {
    pub fn new(
        mut desired: Routes,
        current: Routes,
        saved: Option<Routes>,
        merged_ifaces: &MergedInterfaces,
    ) -> Result<Self, NipartError> {
        desired.remove_ignored_routes();
        desired.validate()?;

        let iface_lists = collect_iface_lists(merged_ifaces);

        let desired_routes =
            resolve_desired_routes(&desired, merged_ifaces, &iface_lists)?;

        let mut changed_ifaces: HashSet<&str> = HashSet::new();

        validate_desired_routes(
            &desired_routes,
            &iface_lists,
            &mut changed_ifaces,
        )?;
        collect_absent_route_changes(
            &desired_routes,
            &current,
            &mut changed_ifaces,
        );

        let mut changed_routes: HashSet<RouteEntry> = HashSet::new();
        let mut merged_routes = build_merged_and_changed_routes(
            &current,
            &desired_routes,
            &iface_lists,
            &mut changed_routes,
        );

        // For interfaces that will be deleted and recreated, current
        // routes are purged by kernel. Include saved routes so they
        // are re-applied along with desired routes.
        if let Some(saved) = saved.as_ref()
            && let Some(saved_rts) = saved.config.as_ref()
        {
            for rt in saved_rts {
                if rt.is_absent() {
                    continue;
                }
                if let Some(via) = rt.next_hop_iface.as_ref()
                    && iface_lists.will_delete.contains(&via.as_str())
                    && !changed_routes.iter().any(|r| rt.is_match(r))
                {
                    changed_routes.insert(rt.clone());
                    merged_routes.push(rt.clone());
                    changed_ifaces.insert(via.as_str());
                }
            }
        }

        let merged = group_by_next_hop_iface(merged_routes);

        let route_changed_ifaces: Vec<String> =
            changed_ifaces.iter().map(|i| i.to_string()).collect();

        let mut ret = Self {
            merged,
            desired,
            current,
            saved,
            route_changed_ifaces,
            changed_routes: changed_routes.drain().collect(),
        };

        ret.remove_routes_to_ignored_ifaces(merged_ifaces);

        Ok(ret)
    }

    fn remove_routes_to_ignored_ifaces(
        &mut self,
        merged_ifaces: &MergedInterfaces,
    ) {
        let ignored_ifaces: Vec<&str> = merged_ifaces
            .kernel_ifaces
            .values()
            .filter_map(|merged_iface| {
                if merged_iface.merged.is_ignore() {
                    Some(merged_iface.merged.kernel_iface_name())
                } else {
                    None
                }
            })
            .collect();

        for iface in ignored_ifaces.as_slice() {
            self.merged.remove(*iface);
        }
        self.route_changed_ifaces
            .retain(|n| !ignored_ifaces.contains(&n.as_str()));
    }

    pub(crate) fn is_changed(&self) -> bool {
        !self.route_changed_ifaces.is_empty()
    }

    pub(crate) fn gen_state_for_apply(&self) -> Routes {
        Routes {
            running: None,
            config: Some(self.changed_routes.clone()),
        }
    }

    pub(crate) fn gen_state_for_save(&self) -> Routes {
        if let Some(config) = self.desired.config.as_ref() {
            Routes {
                running: None,
                config: Some(config.clone()),
            }
        } else if let Some(saved) = self.saved.as_ref()
            && let Some(config) = saved.config.as_ref()
        {
            Routes {
                running: None,
                config: Some(config.clone()),
            }
        } else {
            Routes::default()
        }
    }
}

fn collect_iface_lists(merged_ifaces: &MergedInterfaces) -> IfaceLists<'_> {
    let absent: HashSet<&str> = merged_ifaces
        .kernel_ifaces
        .values()
        .filter(|i| i.merged.is_absent())
        .map(|i| i.merged.kernel_iface_name())
        .collect();

    let ipv4_disabled: HashSet<&str> = merged_ifaces
        .kernel_ifaces
        .values()
        .filter(|i| !i.merged.base_iface().is_ipv4_enabled())
        .map(|i| i.merged.kernel_iface_name())
        .collect();

    let ipv6_disabled: HashSet<&str> = merged_ifaces
        .kernel_ifaces
        .values()
        .filter(|i| !i.merged.base_iface().is_ipv6_enabled())
        .map(|i| i.merged.kernel_iface_name())
        .collect();

    let dhcpv4_enabled: HashSet<&str> = merged_ifaces
        .kernel_ifaces
        .values()
        .filter(|i| {
            i.merged.base_iface().ipv4.as_ref().and_then(|ip| ip.dhcp)
                == Some(true)
        })
        .map(|i| i.merged.kernel_iface_name())
        .collect();

    let will_delete: HashSet<&str> = merged_ifaces
        .kernel_ifaces
        .values()
        .filter(|i| i.will_delete)
        .map(|i| i.merged.kernel_iface_name())
        .collect();

    IfaceLists {
        absent,
        ipv4_disabled,
        ipv6_disabled,
        dhcpv4_enabled,
        will_delete,
    }
}

fn resolve_desired_routes(
    desired: &Routes,
    merged_ifaces: &MergedInterfaces,
    iface_lists: &IfaceLists,
) -> Result<Vec<RouteEntry>, NipartError> {
    let mut desired_routes = Vec::new();
    if let Some(rts) = desired.config.as_ref() {
        for rt in rts {
            let mut rt = rt.clone();
            rt.sanitize()?;
            if let Some(name) = rt.next_hop_iface.as_ref() {
                if let Some(kernel_iface_name) =
                    merged_ifaces.resolve_route_next_hop_iface(name)
                {
                    rt.next_hop_iface = Some(kernel_iface_name);
                } else {
                    return Err(NipartError::new(
                        ErrorKind::InvalidArgument,
                        format!(
                            "Failed to find kernel interface name for route \
                             {rt}"
                        ),
                    ));
                }
            }
            // Kernel rejects IPv4 route with gateway defined before
            // DHCPv4 lease acquired on next hop interface, hence we
            // set onlink flag to bypass kernel gateway validation.
            if rt.onlink.is_none()
                && !rt.is_absent()
                && !rt.is_ipv6()
                && rt.next_hop_addr.is_some()
                && rt
                    .next_hop_iface
                    .as_deref()
                    .is_some_and(|i| iface_lists.dhcpv4_enabled.contains(&i))
            {
                log::debug!(
                    "Setting onlink flag for route '{rt}' as its next hop \
                     interface is DHCPv4 enabled"
                );
                rt.onlink = Some(true);
            }
            desired_routes.push(rt);
        }
    }
    Ok(desired_routes)
}

fn validate_desired_routes<'a>(
    desired_routes: &'a [RouteEntry],
    iface_lists: &IfaceLists<'_>,
    changed_ifaces: &mut HashSet<&'a str>,
) -> Result<(), NipartError> {
    for rt in desired_routes.iter().filter(|rt| !rt.is_absent()) {
        if let Some(via) = rt.next_hop_iface.as_ref() {
            if iface_lists.absent.contains(&via.as_str()) {
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "The next hop interface of desired Route '{rt}' has \
                         been marked as absent"
                    ),
                ));
            }
            if rt.is_ipv6() && iface_lists.ipv6_disabled.contains(&via.as_str())
            {
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "The next hop interface of desired Route '{rt}' has \
                         been marked as IPv6 disabled"
                    ),
                ));
            }
            if (!rt.is_ipv6())
                && iface_lists.ipv4_disabled.contains(&via.as_str())
            {
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "The next hop interface of desired Route '{rt}' has \
                         been marked as IPv4 disabled"
                    ),
                ));
            }
            changed_ifaces.insert(via.as_str());
        } else if rt.route_type.is_some() {
            changed_ifaces.insert(LOOPBACK_IFACE_NAME);
        }
    }
    Ok(())
}

fn collect_absent_route_changes<'a>(
    desired_routes: &[RouteEntry],
    current: &'a Routes,
    changed_ifaces: &mut HashSet<&'a str>,
) {
    for absent_rt in desired_routes.iter().filter(|rt| rt.is_absent()) {
        if let Some(cur_rts) = current.config.as_ref() {
            for rt in cur_rts {
                if absent_rt.is_match(rt) {
                    if let Some(via) = rt.next_hop_iface.as_ref() {
                        changed_ifaces.insert(via.as_str());
                    } else {
                        changed_ifaces.insert(LOOPBACK_IFACE_NAME);
                    }
                }
            }
        }
    }
}

fn build_merged_and_changed_routes(
    current: &Routes,
    desired_routes: &[RouteEntry],
    iface_lists: &IfaceLists,
    changed_routes: &mut HashSet<RouteEntry>,
) -> Vec<RouteEntry> {
    let mut merged_routes: Vec<RouteEntry> = Vec::new();

    if let Some(cur_rts) = current.config.as_ref() {
        for rt in cur_rts {
            if let Some(via) = rt.next_hop_iface.as_ref() {
                if iface_lists.will_delete.contains(&via.as_str()) {
                    // Routes on will_delete interfaces will be purged
                    // by kernel when the interface is deleted, skip
                    // them from current so they get re-applied.
                    continue;
                }
                if iface_lists.absent.contains(&via.as_str())
                    || (rt.is_ipv6()
                        && iface_lists.ipv6_disabled.contains(&via.as_str()))
                    || (!rt.is_ipv6()
                        && iface_lists.ipv4_disabled.contains(&via.as_str()))
                    || desired_routes
                        .iter()
                        .filter(|r| r.is_absent())
                        .any(|absent_rt| absent_rt.is_match(rt))
                {
                    let mut new_rt = rt.clone();
                    new_rt.state = Some(RouteState::Absent);
                    changed_routes.insert(new_rt);
                } else {
                    merged_routes.push(rt.clone());
                }
            }
        }
    }

    for rt in desired_routes.iter().filter(|rt| !rt.is_absent()) {
        let is_will_delete_iface = rt
            .next_hop_iface
            .as_deref()
            .is_some_and(|via| iface_lists.will_delete.contains(via));
        if is_will_delete_iface {
            // Current routes on this interface are purged, so always
            // treat desired routes as new.
            changed_routes.insert(rt.clone());
            merged_routes.push(rt.clone());
        } else if let Some(cur_rts) = current.config.as_ref() {
            if !cur_rts.iter().any(|cur_rt| rt.is_match(cur_rt)) {
                changed_routes.insert(rt.clone());
                merged_routes.push(rt.clone());
            }
        } else {
            changed_routes.insert(rt.clone());
            merged_routes.push(rt.clone());
        }
    }

    merged_routes.sort_unstable();
    merged_routes.dedup();
    merged_routes
}

fn group_by_next_hop_iface(
    merged_routes: Vec<RouteEntry>,
) -> HashMap<String, Vec<RouteEntry>> {
    let mut merged: HashMap<String, Vec<RouteEntry>> = HashMap::new();
    for rt in merged_routes {
        if let Some(via) = rt.next_hop_iface.as_ref() {
            let rts: &mut Vec<RouteEntry> = match merged.entry(via.to_string())
            {
                Entry::Occupied(o) => o.into_mut(),
                Entry::Vacant(v) => v.insert(Vec::new()),
            };
            rts.push(rt);
        } else if rt.route_type.is_some() {
            let rts: &mut Vec<RouteEntry> =
                match merged.entry(LOOPBACK_IFACE_NAME.to_string()) {
                    Entry::Occupied(o) => o.into_mut(),
                    Entry::Vacant(v) => v.insert(Vec::new()),
                };
            rts.push(rt);
        }
    }
    merged
}

impl Routes {
    /// Return new Routes data contains the merged data.
    pub(crate) fn merge(&self, new_routes: &Self) -> Result<Self, NipartError> {
        new_routes.validate()?;

        if let Some(new_routes) = new_routes.config.as_ref() {
            let mut route_sets: HashSet<RouteEntry> = HashSet::new();
            for new_route in new_routes.iter().filter(|r| !r.is_absent()) {
                route_sets.insert(new_route.clone());
            }
            if let Some(old_routes) = self.config.as_ref() {
                for old_route in old_routes {
                    if new_routes
                        .iter()
                        .any(|r| r.is_absent() && r.is_match(old_route))
                    {
                        let mut absent_route = old_route.clone();
                        absent_route.state = Some(RouteState::Absent);
                        route_sets.insert(absent_route);
                    } else {
                        route_sets.insert(old_route.clone());
                    }
                }
            }
            let mut routes: Vec<RouteEntry> = route_sets.into_iter().collect();
            routes.sort_unstable();

            Ok(Routes {
                config: Some(routes),
                ..Default::default()
            })
        } else {
            Ok(self.clone())
        }
    }
}
