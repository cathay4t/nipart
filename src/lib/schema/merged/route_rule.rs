// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nmstate project(https://nmstate.io/) which
// is under license of Apache 2.0, authors of original file are:
//  * Gris Ge <fge@redhat.com>
//  * Wen Liang <liangwen12year@gmail.com>
//  * Jan Vaclav <jvaclav@redhat.com>
//  * Íñigo Huguet <ihuguet@redhat.com>
//  * Fernando Fernandez Mancera <ffmancera@riseup.net>

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::super::route_rule::set_auto_priority;
use crate::{
    ErrorKind, JsonDisplay, MergedInterfaces, NipartError, NipartInterface,
    RouteRuleEntry, RouteRuleState, RouteRules,
};

#[derive(
    Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonDisplay,
)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub struct MergedRouteRules {
    pub desired: RouteRules,
    pub current: RouteRules,
    // The `changed_rules` holds two kinds of route rule:
    //  * Desired new route rules
    //  * Current route rules marked as absent
    pub changed_rules: Vec<RouteRuleEntry>,
    #[serde(default)]
    pub(crate) for_save: RouteRules,
    // Desired rules with `iif` resolved to kernel interface names, used for
    // verification.
    #[serde(default)]
    pub(crate) for_verify: Vec<RouteRuleEntry>,
}

impl MergedRouteRules {
    pub fn new(
        desired: RouteRules,
        current: RouteRules,
        saved: Option<RouteRules>,
        merged_ifaces: &MergedInterfaces,
    ) -> Result<Self, NipartError> {
        desired.validate()?;
        current.validate()?;

        let absent_ifaces: HashSet<String> = merged_ifaces
            .kernel_ifaces
            .values()
            .filter(|merged_iface| merged_iface.merged.is_absent())
            .map(|merged_iface| {
                merged_iface.merged.kernel_iface_name().to_string()
            })
            .collect();

        let mut desired_rules: Vec<RouteRuleEntry> = Vec::new();
        let mut desired_absent_rules: Vec<RouteRuleEntry> = Vec::new();
        if let Some(rules) = desired.config.as_ref() {
            for rule in rules {
                let mut new_rule = rule.clone();
                if new_rule.is_absent() {
                    resolve_rule_iif(&mut new_rule, merged_ifaces);
                    desired_absent_rules.push(new_rule);
                } else {
                    new_rule.sanitize()?;
                    desired_rules.push(new_rule);
                }
            }
        }

        let mut saved_rules: Vec<RouteRuleEntry> = Vec::new();
        if let Some(saved_config) =
            saved.as_ref().and_then(|saved| saved.config.as_ref())
        {
            for rule in saved_config.iter().filter(|rule| !rule.is_absent()) {
                let mut new_rule = rule.clone();
                new_rule.sanitize()?;
                saved_rules.push(new_rule);
            }
        }

        // Inherit undefined properties of a desired rule from the saved rule
        // it matches, so a partial re-apply keeps the effective settings
        // (e.g. the auto assigned priority) instead of persisting the same
        // rule twice or re-creating it with different settings after reboot.
        let mut consumed_saved_rules: HashSet<usize> = HashSet::new();
        for desired_rule in desired_rules.iter_mut() {
            let mut resolved_desired = desired_rule.clone();
            resolve_rule_iif(&mut resolved_desired, merged_ifaces);
            for (saved_index, saved_rule) in saved_rules.iter().enumerate() {
                if consumed_saved_rules.contains(&saved_index) {
                    continue;
                }
                let mut resolved_saved = saved_rule.clone();
                resolve_rule_iif(&mut resolved_saved, merged_ifaces);
                if resolved_desired.is_match(&resolved_saved) {
                    fill_undefined_rule_fields(desired_rule, saved_rule);
                    consumed_saved_rules.insert(saved_index);
                    break;
                }
            }
        }

        // Rules sent to the kernel and checked by verification must reference
        // their interface by kernel name: `iif` may hold a profile name.
        let mut for_verify: Vec<RouteRuleEntry> = desired_rules.clone();
        for rule in for_verify.iter_mut() {
            resolve_rule_iif(rule, merged_ifaces);
        }

        let mut changed_rules: HashSet<RouteRuleEntry> = HashSet::new();
        let mut current_rules: Vec<RouteRuleEntry> = Vec::new();
        if let Some(rules) = current.config.as_ref() {
            for rule in rules {
                current_rules.push(rule.clone());
                let removed_by_desired_absent = desired_absent_rules
                    .iter()
                    .any(|absent_rule| absent_rule.is_match(rule));
                // The kernel keeps `iif` rules detached when their interface
                // is deleted, so remove them explicitly.
                let iface_deleted = rule
                    .iif
                    .as_deref()
                    .is_some_and(|iif| absent_ifaces.contains(iif));
                if removed_by_desired_absent || iface_deleted {
                    let mut absent_rule = rule.clone();
                    absent_rule.state = Some(RouteRuleState::Absent);
                    absent_rule.sanitize()?;
                    changed_rules.insert(absent_rule);
                }
            }
        }

        for desired_rule in &for_verify {
            if !current_rule_matches(&current_rules, desired_rule) {
                changed_rules.insert(desired_rule.clone());
            }
        }

        let mut changed_rules: Vec<RouteRuleEntry> =
            changed_rules.into_iter().collect();
        changed_rules.sort_unstable();
        set_auto_priority(&mut changed_rules, &current_rules);

        let for_save = gen_route_rules_for_save(
            &desired_rules,
            &desired_absent_rules,
            &saved_rules,
            &consumed_saved_rules,
            &absent_ifaces,
            merged_ifaces,
        )?;

        Ok(Self {
            desired,
            current,
            changed_rules,
            for_save,
            for_verify,
        })
    }

    pub(crate) fn is_changed(&self) -> bool {
        !self.changed_rules.is_empty()
    }

    pub(crate) fn gen_state_for_apply(&self) -> RouteRules {
        RouteRules {
            config: if self.changed_rules.is_empty() {
                None
            } else {
                Some(self.changed_rules.clone())
            },
        }
    }

    pub(crate) fn gen_state_for_save(&self) -> RouteRules {
        RouteRules {
            config: self.for_save.config.clone(),
        }
    }

    pub(crate) fn gen_diff(&self) -> RouteRules {
        self.gen_state_for_apply()
    }

    pub(crate) fn generate_revert(&self) -> Result<RouteRules, NipartError> {
        let mut revert_rules: Vec<RouteRuleEntry> = Vec::new();
        let empty_vec: Vec<RouteRuleEntry> = Vec::new();
        let current_rules = self.current.config.as_ref().unwrap_or(&empty_vec);

        for changed_rule in self.changed_rules.iter() {
            if changed_rule.is_absent() {
                for cur_rule in current_rules {
                    if changed_rule.is_match(cur_rule) {
                        revert_rules.push(cur_rule.clone());
                    }
                }
            } else {
                let mut revert_rule = changed_rule.clone();
                revert_rule.state = Some(RouteRuleState::Absent);
                revert_rules.push(revert_rule);
            }
        }

        revert_rules.sort_unstable();
        revert_rules.dedup();
        Ok(RouteRules {
            config: if revert_rules.is_empty() {
                None
            } else {
                Some(revert_rules)
            },
        })
    }

    pub(crate) fn verify(
        &self,
        current: &RouteRules,
    ) -> Result<(), NipartError> {
        for desired_rule in self.for_verify.iter() {
            if !current_rule_matches(
                current.config.as_deref().unwrap_or_default(),
                desired_rule,
            ) {
                return Err(NipartError::new(
                    ErrorKind::VerificationError,
                    format!(
                        "Desired route rule {desired_rule} not found after \
                         apply"
                    ),
                ));
            }
        }

        for absent_rule in
            self.changed_rules.iter().filter(|rule| rule.is_absent())
        {
            if current.config.as_ref().is_some_and(|rules| {
                rules.iter().any(|r| absent_rule.is_match(r))
            }) {
                return Err(NipartError::new(
                    ErrorKind::VerificationError,
                    format!(
                        "Desired absent route rule {absent_rule} still found \
                         after apply"
                    ),
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn remove_rules_to_ignored_ifaces(
        &mut self,
        ignored_ifaces: &[&str],
    ) {
        let retains_rule = |rule: &RouteRuleEntry| {
            rule.iif
                .as_ref()
                .is_none_or(|iif| !ignored_ifaces.contains(&iif.as_str()))
        };
        if let Some(rules) = self.desired.config.as_mut() {
            rules.retain(retains_rule);
        }
        if let Some(rules) = self.for_save.config.as_mut() {
            rules.retain(retains_rule);
        }
        self.changed_rules.retain(retains_rule);
        self.for_verify.retain(retains_rule);
    }
}

/// Resolve the `iif` profile or logical name of a rule to the kernel
/// interface name. Rules referencing an interface by name are left
/// unchanged, and unresolvable names (interface not present) are kept so the
/// rule stays pending.
fn resolve_rule_iif(
    rule: &mut RouteRuleEntry,
    merged_ifaces: &MergedInterfaces,
) {
    if let Some(iif) = rule.iif.as_deref()
        && let Some(kernel_iface_name) =
            merged_ifaces.resolve_route_next_hop_iface(iif)
        && kernel_iface_name != iif
    {
        log::debug!(
            "Route rule iif {iif} resolved to kernel interface \
             {kernel_iface_name}"
        );
        rule.iif = Some(kernel_iface_name);
    }
}

/// Fill properties undefined in `rule` with the value from its matching
/// saved `saved` rule.
fn fill_undefined_rule_fields(
    rule: &mut RouteRuleEntry,
    saved: &RouteRuleEntry,
) {
    if rule.family.is_none() {
        rule.family = saved.family;
    }
    if rule.ip_from.is_none() {
        rule.ip_from.clone_from(&saved.ip_from);
    }
    if rule.ip_to.is_none() {
        rule.ip_to.clone_from(&saved.ip_to);
    }
    if rule.priority.is_none()
        || rule.priority == Some(RouteRuleEntry::USE_DEFAULT_PRIORITY)
    {
        rule.priority = saved.priority;
    }
    if rule.table_id.is_none() {
        rule.table_id = saved.table_id;
    }
    if rule.fwmark.is_none() {
        rule.fwmark = saved.fwmark;
    }
    if rule.fwmask.is_none() {
        rule.fwmask = saved.fwmask;
    }
    if rule.action.is_none() {
        rule.action = saved.action;
    }
    if rule.iif.is_none() {
        rule.iif.clone_from(&saved.iif);
    }
    if rule.suppress_prefix_length.is_none() {
        rule.suppress_prefix_length = saved.suppress_prefix_length;
    }
}

fn current_rule_matches(
    current_rules: &[RouteRuleEntry],
    rule: &RouteRuleEntry,
) -> bool {
    current_rules.iter().any(|cur_rule| rule.is_match(cur_rule))
}

/// Compute the route rules to persist: the desired non-absent rules plus
/// every previously saved route rule that survives this apply. A saved rule
/// matched by a desired rule is dropped: the desired rule (with properties
/// inherited from the saved one) is persisted instead. A saved rule matched
/// by a desired absent rule, or bound to an interface deleted by this apply,
/// is dropped.
fn gen_route_rules_for_save(
    desired_rules: &[RouteRuleEntry],
    desired_absent_rules: &[RouteRuleEntry],
    saved_rules: &[RouteRuleEntry],
    consumed_saved_rules: &HashSet<usize>,
    absent_ifaces: &HashSet<String>,
    merged_ifaces: &MergedInterfaces,
) -> Result<RouteRules, NipartError> {
    let mut rules: HashSet<RouteRuleEntry> = HashSet::new();
    for new_rule in desired_rules {
        rules.insert(new_rule.clone());
    }
    for (saved_index, saved_rule) in saved_rules.iter().enumerate() {
        if consumed_saved_rules.contains(&saved_index) {
            continue;
        }
        let mut resolved_saved = saved_rule.clone();
        resolve_rule_iif(&mut resolved_saved, merged_ifaces);
        let removed = desired_absent_rules
            .iter()
            .any(|desired_rule| desired_rule.is_match(&resolved_saved))
            || resolved_saved
                .iif
                .as_deref()
                .is_some_and(|iif| absent_ifaces.contains(iif));
        if !removed {
            rules.insert(saved_rule.clone());
        }
    }

    let mut rules: Vec<RouteRuleEntry> = rules.into_iter().collect();
    rules.sort_unstable();
    Ok(RouteRules {
        config: if rules.is_empty() { None } else { Some(rules) },
    })
}
