// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nmstate project(https://nmstate.io/) which
// is under license of Apache 2.0, authors of original file are:
//  * Gris Ge <fge@redhat.com>
//  * Wen Liang <liangwen12year@gmail.com>
//  * Íñigo Huguet <ihuguet@redhat.com>

use crate::{
    AddressFamily, ErrorKind, NipartError, RouteRuleAction, RouteRuleEntry,
    RouteRules,
};

// The kernel adds `ip rule` entries with protocol boot by default, while
// NetworkManager used protocol unspec historically.  Include both plus the
// static protocol when presenting rules as manageable configuration.
const SUPPORTED_STATIC_ROUTE_RULE_PROTOCOL: [nispor::RouteProtocol; 3] = [
    nispor::RouteProtocol::Boot,
    nispor::RouteProtocol::Static,
    nispor::RouteProtocol::Unspec,
];

pub(crate) fn get_route_rules(np_rules: &[nispor::RouteRule]) -> RouteRules {
    let mut rules = Vec::new();
    for np_rule in np_rules {
        let mut rule = RouteRuleEntry::new();
        match np_rule.action {
            nispor::RuleAction::Table => (),
            nispor::RuleAction::Blackhole => {
                rule.action = Some(RouteRuleAction::Blackhole)
            }
            nispor::RuleAction::Unreachable => {
                rule.action = Some(RouteRuleAction::Unreachable)
            }
            nispor::RuleAction::Prohibit => {
                rule.action = Some(RouteRuleAction::Prohibit)
            }
            _ => {
                log::debug!("Got unsupported route rule {np_rule:?}");
                continue;
            }
        }
        if let Some(rule_protocol) = np_rule.protocol.as_ref()
            && !SUPPORTED_STATIC_ROUTE_RULE_PROTOCOL.contains(rule_protocol)
        {
            continue;
        }
        rule.iif.clone_from(&np_rule.iif);
        rule.ip_to.clone_from(&np_rule.dst);
        rule.ip_from.clone_from(&np_rule.src);
        rule.table_id = np_rule.table;
        rule.priority = np_rule.priority.map(i64::from);
        rule.fwmark = np_rule.fw_mark;
        rule.fwmask = np_rule.fw_mask;
        rule.suppress_prefix_length = np_rule.suppress_prefix_len;
        rule.family = match np_rule.address_family {
            nispor::AddressFamily::Ipv4 => Some(AddressFamily::Ipv4),
            nispor::AddressFamily::Ipv6 => Some(AddressFamily::Ipv6),
            _ => {
                log::warn!(
                    "Unsupported route rule family {:?}",
                    np_rule.address_family
                );
                None
            }
        };
        rules.push(rule);
    }
    rules.sort_unstable();
    rules.dedup();
    RouteRules {
        config: Some(rules),
    }
}

pub(crate) async fn apply_route_rules(
    merged_rules: &crate::MergedRouteRules,
) -> Result<(), NipartError> {
    if !merged_rules.is_changed() {
        log::debug!("Route rule is not changed");
        return Ok(());
    }

    let mut rule_confs: Vec<nispor::RouteRuleConf> = Vec::new();
    // Remove absent rules before adding new ones, so a rule replaced in a
    // single transaction never conflicts with its old entry.
    for rule in merged_rules
        .changed_rules
        .iter()
        .filter(|rule| rule.is_absent())
    {
        log::debug!("Removing route rule {rule}");
        rule_confs.push(nipart_to_nispor_rule_conf(rule)?);
    }
    for rule in merged_rules
        .changed_rules
        .iter()
        .filter(|rule| !rule.is_absent())
    {
        log::debug!("Adding route rule {rule}");
        rule_confs.push(nipart_to_nispor_rule_conf(rule)?);
    }

    let mut net_conf = nispor::NetConf::default();
    net_conf.rules = Some(rule_confs);
    log::trace!(
        "Pending kernel route rule changes {}",
        serde_json::to_string(&net_conf).unwrap_or_default()
    );
    if let Err(e) = net_conf.apply_async().await {
        return Err(NipartError::new(
            ErrorKind::Bug,
            format!("Failed to change route rules: {e}"),
        ));
    }
    Ok(())
}

fn nipart_to_nispor_rule_conf(
    rule: &RouteRuleEntry,
) -> Result<nispor::RouteRuleConf, NipartError> {
    let Some(priority) = rule.priority else {
        return Err(NipartError::new(
            ErrorKind::InvalidArgument,
            format!("Route rule {rule} has no priority defined"),
        ));
    };
    let priority = u32::try_from(priority).map_err(|_| {
        NipartError::new(
            ErrorKind::InvalidArgument,
            format!(
                "Invalid route rule priority {priority}, expecting a \
                 non-negative integer no greater than {}",
                u32::MAX
            ),
        )
    })?;

    let mut conf = nispor::RouteRuleConf::default();
    conf.remove = rule.is_absent();
    conf.address_family = if rule.is_ipv6() {
        nispor::AddressFamily::Ipv6
    } else {
        nispor::AddressFamily::Ipv4
    };
    conf.action = match rule.action {
        Some(RouteRuleAction::Blackhole) => nispor::RuleAction::Blackhole,
        Some(RouteRuleAction::Unreachable) => nispor::RuleAction::Unreachable,
        Some(RouteRuleAction::Prohibit) => nispor::RuleAction::Prohibit,
        None => nispor::RuleAction::Table,
    };
    conf.table = rule.table_id.filter(|table_id| {
        *table_id != RouteRuleEntry::USE_DEFAULT_ROUTE_TABLE
    });
    conf.src = rule.ip_from.clone();
    conf.dst = rule.ip_to.clone();
    conf.iif = rule.iif.clone();
    conf.priority = priority;
    conf.fw_mark = rule.fwmark;
    conf.fw_mask = rule.fwmask;
    conf.suppress_prefix_len = rule.suppress_prefix_length;
    Ok(conf)
}
