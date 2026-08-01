// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nmstate project(https://nmstate.io/) which
// is under license of Apache 2.0, authors of original file are:
//  * Gris Ge <fge@redhat.com>
//  * Fernando Fernandez Mancera <ffmancera@riseup.net>
//  * Mat Kowalski <mko@redhat.com>

use serde::{Deserialize, Serialize};

use crate::{
    ErrorKind, InterfaceIdentifier, InterfaceIpv4, InterfaceIpv6,
    InterfaceLinkState, InterfaceState, InterfaceTrigger, InterfaceType,
    JsonDisplay, NipartError,
};

#[derive(
    Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonDisplay,
)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[non_exhaustive]
/// Information shared among all interface types
pub struct BaseInterface {
    pub name: String,
    /// The actual kernel interface name. When using
    /// [InterfaceIdentifier::MacAddress], this holds the resolved kernel
    /// interface name. When [InterfaceIdentifier::Name] or undefined,
    /// this is copied from `name` during sanitization.
    /// For `[InterfaceIdentifier::Name]` (default), if this property
    /// undefined during apply, this will take `name` property.
    /// Serialize and deserialize to/from `kernel-iface-name`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kernel_iface_name: String,
    /// Profile name to differentiate from the actual interface name when using
    /// [InterfaceIdentifier::MacAddress].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
    // TODO: To simplify the code and workflow, we ask user to always define
    //       interface type. Let's heard feedback or use case to release
    //       this restriction.
    #[serde(rename = "type")]
    pub iface_type: InterfaceType,
    /// Define network backend matching method on choosing network interface.
    /// Default to [InterfaceIdentifier::Name].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier: Option<InterfaceIdentifier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iface_index: Option<u32>,
    #[serde(default)]
    pub state: InterfaceState,
    /// Bring interface up and down base on [InterfaceTrigger] instruction.
    /// Default value is `[InterfaceTrigger::OneShot]` meaning daemon use
    /// `state` value to bring interface up/down and never take any auto action
    /// afterwards.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<InterfaceTrigger>,
    /// Link carrier state, query only property.
    /// Serialize to `link-state`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_state: Option<InterfaceLinkState>,
    /// Controller interface name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller_type: Option<InterfaceType>,
    /// MAC address in the format: upper case hex string separated by `:` on
    /// every two characters. Case insensitive when applying.
    /// When applying, this property is only used with
    /// `InterfaceIdentifier::MacAddress`, otherwise, silently ignored.
    /// Serialize and deserialize to/from `mac-address`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac_address: Option<String>,
    /// MAC address never change after reboots(normally stored in firmware of
    /// network interface). Using the same format as `mac_address` property.
    /// Ignored during apply.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permanent_mac_address: Option<String>,
    /// Maximum transmission unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u64>,
    /// Minimum MTU allowed. Ignored during apply.
    /// Serialize and deserialize to/from `min-mtu`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_mtu: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Maximum MTU allowed. Ignored during apply.
    /// Serialize and deserialize to/from `max-mtu`.
    pub max_mtu: Option<u64>,
    /// IPv4 information.
    /// Hided if interface is not allowed to hold IP information(e.g. port of
    /// bond is not allowed to hold IP information).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv4: Option<InterfaceIpv4>,
    /// IPv4 information.
    /// Hided if interface is not allowed to hold IP information(e.g. port of
    /// bond is not allowed to hold IP information).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv6: Option<InterfaceIpv6>,
}

impl BaseInterface {
    pub fn sanitize(
        &self,
        current: Option<&Self>,
        for_save: &mut Self,
        for_apply: &mut Self,
        for_verify: &mut Self,
        merged: &mut Self,
    ) -> Result<(), NipartError> {
        let desired = self;
        // Ignore MAC address if not InterfaceIdentifier::MacAddress
        if for_apply.identifier != Some(InterfaceIdentifier::MacAddress)
            && for_apply.mac_address.is_some()
            && for_apply.is_up()
        {
            log::info!(
                "Ignoring interface {}/{} MAC address property for apply \
                 action because it is not `identifier: mac-address`",
                for_apply.name,
                for_apply.iface_type,
            );
            for_apply.mac_address = None;
            for_verify.mac_address = None;
        }

        if let Some(mac) = for_apply.mac_address.as_ref() {
            for_apply.mac_address = Some(mac.to_uppercase());
        }
        // permanent_mac_address is query-only, should not be in for_apply
        for_apply.permanent_mac_address = None;
        for_save.permanent_mac_address = None;
        for_verify.permanent_mac_address = None;

        if let Some(ipv4) = for_apply.ipv4.as_mut() {
            ipv4.sanitize(current.and_then(|c| c.ipv4.as_ref()))?;
            for_save.ipv4 = Some(ipv4.clone());
            for_verify.ipv4 = Some(ipv4.clone());
        }
        if let Some(ipv6) = for_apply.ipv6.as_mut() {
            ipv6.sanitize(current.and_then(|c| c.ipv6.as_ref()))?;
            for_save.ipv6 = Some(ipv6.clone());
            for_verify.ipv6 = Some(ipv6.clone());
        }
        if let Some(mac) = for_apply.mac_address.as_ref() {
            for_save.mac_address = Some(mac.to_string());
            for_verify.mac_address = Some(mac.to_string());
            merged.mac_address = Some(mac.to_string());
        }
        if let Some(mac) = merged.permanent_mac_address.as_ref() {
            merged.permanent_mac_address = Some(mac.to_uppercase());
        }

        if let Some(cur_kernel_iface_name) =
            current.as_ref().map(|c| c.kernel_iface_name.as_str())
            && !cur_kernel_iface_name.is_empty()
            && for_apply.kernel_iface_name.is_empty()
        {
            for_apply.kernel_iface_name = cur_kernel_iface_name.to_string();
        }

        if !for_apply.is_name_matching() {
            for_save.kernel_iface_name = String::new();
            if let Some(profile_name) = for_save.profile_name.as_ref()
                && !profile_name.is_empty()
            {
                for_save.name.clone_from(profile_name);
            }
        }

        // iface_index is query only
        for_save.iface_index = None;
        for_apply.iface_index = None;

        desired.validate_mtu(current)?;
        if desired.identifier == Some(InterfaceIdentifier::MacAddress)
            && desired.iface_type != InterfaceType::Ethernet
        {
            return Err(NipartError::new(
                ErrorKind::InvalidArgument,
                format!(
                    "The `identifier: mac-address` is only supported on \
                     ethernet interfaces, but interface {} is of type {}",
                    desired.name, desired.iface_type
                ),
            ));
        }
        Ok(())
    }

    fn validate_mtu(&self, current: Option<&Self>) -> Result<(), NipartError> {
        let desired = self;
        if let Some(current) = current
            && let (Some(desire_mtu), Some(min_mtu), Some(max_mtu)) =
                (desired.mtu, current.min_mtu, current.max_mtu)
        {
            if desire_mtu > max_mtu {
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "Desired MTU {} for interface {} is bigger than \
                         maximum allowed MTU {}",
                        desire_mtu, desired.name, max_mtu
                    ),
                ));
            } else if desire_mtu < min_mtu {
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "Desired MTU {} for interface {} is smaller than \
                         minimum allowed MTU {}",
                        desire_mtu, desired.name, min_mtu
                    ),
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn sanitize_before_verify(&mut self, current: &mut Self) {
        self.identifier = None;
        self.profile_name = None;
        self.kernel_iface_name.clear();
        self.controller_type = None;
        if let Some(des_ipv4) = self.ipv4.as_mut()
            && let Some(cur_ipv4) = current.ipv4.as_mut()
        {
            des_ipv4.sanitize_before_verify(cur_ipv4);
        }
        if let Some(des_ipv6) = self.ipv6.as_mut()
            && let Some(cur_ipv6) = current.ipv6.as_mut()
        {
            des_ipv6.sanitize_before_verify(cur_ipv6);
        }
    }

    pub fn clone_name_type_only(&self) -> Self {
        Self {
            name: self.name.clone(),
            iface_type: self.iface_type.clone(),
            state: InterfaceState::Up,
            kernel_iface_name: self.kernel_iface_name.clone(),
            ..Default::default()
        }
    }

    pub(crate) fn is_up(&self) -> bool {
        self.state == InterfaceState::Up
    }

    pub(crate) fn is_absent(&self) -> bool {
        self.state == InterfaceState::Absent
    }

    pub(crate) fn is_userspace(&self) -> bool {
        self.iface_type.is_userspace()
    }

    fn has_controller(&self) -> bool {
        if let Some(ctrl) = self.controller.as_deref() {
            !ctrl.is_empty()
        } else {
            false
        }
    }

    pub(crate) fn is_ipv4_enabled(&self) -> bool {
        self.ipv4.as_ref().map(|i| i.is_enabled()) == Some(true)
    }

    pub(crate) fn is_ipv6_enabled(&self) -> bool {
        self.ipv6.as_ref().map(|i| i.is_enabled()) == Some(true)
    }

    pub(crate) fn need_controller(&self) -> bool {
        self.iface_type.need_controller()
    }

    /// Whether this interface can hold IP information or not.
    pub(crate) fn can_have_ip(&self) -> bool {
        (!self.has_controller())
            || self.iface_type == InterfaceType::OvsInterface
            || self.controller_type == Some(InterfaceType::Vrf)
    }

    pub fn is_name_matching(&self) -> bool {
        matches!(self.identifier, None | Some(InterfaceIdentifier::Name))
    }

    pub fn new(name: String, iface_type: InterfaceType) -> Self {
        Self {
            name,
            iface_type,
            state: InterfaceState::Up,
            ..Default::default()
        }
    }
}
