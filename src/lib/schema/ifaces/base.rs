// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nmstate project(https://nmstate.io/) which
// is under license of Apache 2.0, authors of original file are:
//  * Gris Ge <fge@redhat.com>
//  * Fernando Fernandez Mancera <ffmancera@riseup.net>
//  * Mat Kowalski <mko@redhat.com>

use serde::{Deserialize, Serialize};

use crate::{
    AltNameEntry, ErrorKind, InterfaceAutoConnect, InterfaceIdentifier,
    InterfaceIpv4, InterfaceIpv6, InterfaceLinkState, InterfaceState,
    InterfaceType, JsonDisplay, NipartError,
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
    /// Interface description. This is a save-only property: it is persisted
    /// in the saved state but never applied to the kernel or checked during
    /// verification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
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
    /// Bring interface up and down base on [InterfaceAutoConnect]
    /// instruction.
    /// Default value is [InterfaceAutoConnect::AutoConnect] meaning daemon
    /// automatically activates the interface: for physical interface, the
    /// config is applied upon carrier up, for virtual interface, the config
    /// is applied upon boot or apply action.
    /// When [InterfaceAutoConnect::Manual], the config is only applied upon
    /// apply action, ignored in boot action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_connect: Option<InterfaceAutoConnect>,
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
    /// Alternative names of the interface. An entry with `state: absent`
    /// removes that alternative name.
    /// Serialize and deserialize to/from `alt-names`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt_names: Option<Vec<AltNameEntry>>,
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
        saved: Option<&Self>,
        for_save: &mut Self,
        for_apply: &mut Self,
        for_verify: &mut Self,
        merged: &mut Self,
    ) -> Result<(), NipartError> {
        let desired = self;
        self.fill_for_apply_ip_config(current, saved, for_save, for_apply);
        // `description` is save-only, never applied to kernel nor verified.
        for_apply.description = None;
        for_verify.description = None;
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
        }
        if let Some(ipv6) = for_apply.ipv6.as_mut() {
            ipv6.sanitize(current.and_then(|c| c.ipv6.as_ref()))?;
        }
        // `for_verify` is a clone of the desired state: sanitize it in place
        // so verification only checks what the user requested. Copying the
        // full merged config from `for_apply` here would verify properties
        // inherited from `saved` (e.g. `enabled: true` while the kernel
        // reports `enabled: false` because no address is assigned), failing
        // apply of otherwise valid configs (e.g. switching DHCP off without
        // specifying addresses).
        if let Some(ipv4) = for_verify.ipv4.as_mut() {
            ipv4.sanitize(current.and_then(|c| c.ipv4.as_ref()))?;
        }
        if let Some(ipv6) = for_verify.ipv6.as_mut() {
            ipv6.sanitize(current.and_then(|c| c.ipv6.as_ref()))?;
        }
        // `for_apply` only holds the diff between the merged and current
        // states, so its ipv4/ipv6 must not be copied onto `for_save` — that
        // would purge the untouched parts of the saved IP config (e.g. static
        // addresses). Sanitize the full `for_save` IP config in place instead.
        if let Some(ipv4) = for_save.ipv4.as_mut() {
            ipv4.sanitize(current.and_then(|c| c.ipv4.as_ref()))?;
        }
        if let Some(ipv6) = for_save.ipv6.as_mut() {
            ipv6.sanitize(current.and_then(|c| c.ipv6.as_ref()))?;
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
            // For a MAC-identified config, the saved state keeps the
            // profile name as `name` and (unless the user explicitly
            // requested a rename via `kernel-iface-name`) clears the
            // kernel name so the config stays resolvable by MAC at boot.
            // The desired `kernel_iface_name` holds the resolved kernel
            // name: it only differs from the current kernel name when the
            // user requested an explicit rename.
            if current.is_none()
                || current.map(|c| c.kernel_iface_name.as_str())
                    == Some(self.kernel_iface_name.as_str())
            {
                for_save.kernel_iface_name = String::new();
            }
            if let Some(profile_name) = for_save.profile_name.as_ref()
                && !profile_name.is_empty()
            {
                for_save.name.clone_from(profile_name);
            }
        }

        // iface_index is query only
        for_save.iface_index = None;
        for_apply.iface_index = None;

        // Alternative names: sorted so the verification (which compares the
        // list in order) matches the kernel-reported order after apply.
        for_apply.sanitize_alt_names();
        for_save.sanitize_alt_names();
        for_verify.sanitize_alt_names();
        merged.sanitize_alt_names();

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

    /// `for_apply` is generated as a diff against the current kernel state
    /// (see [Interface::gen_diff]), so its `ipv4`/`ipv6` only carry the
    /// *changed* properties. The apply action needs the full desired IP
    /// config instead: `self` (the unchanged desired state) merged with the
    /// previous state (`saved`, or `current` when there is no saved config),
    /// so the backend can transition the interface correctly:
    ///  * previous static → desired auto (`dhcp: true`/`autoconf: true`):
    ///    discard the previous static addresses;
    ///  * previous auto → desired `dhcp: false` + `autoconf: false`: replace
    ///    the dynamic addresses with the desired ones; when the desired state
    ///    does not specify addresses, IPv4 gets no IP and IPv6 keeps only the
    ///    kernel-generated link-local address.
    ///
    /// When the saved config exists, the same adjusted full config is also
    /// stored into `for_save` so the persisted state never keeps stale
    /// addresses from a mode switch.
    fn fill_for_apply_ip_config(
        &self,
        current: Option<&Self>,
        saved: Option<&Self>,
        for_save: &mut Self,
        for_apply: &mut Self,
    ) {
        let desired = self;
        let prev_iface = saved.or(current);

        // Whether the diff state carries an IPv4/IPv6 change at all.
        let ipv4_changed = for_apply.ipv4.is_some();
        let ipv6_changed = for_apply.ipv6.is_some();

        // The full merged IP config: `desired` merged with the previous
        // state. With a saved config, `for_save` (= saved ∪ desired) already
        // holds the full config. Without a saved config, only the `addresses`
        // of the current kernel state are inherited when the desired state
        // leaves them undefined; `enabled`/`dhcp`/`autoconf` are never
        // inherited from the kernel state — undefined `enabled` means
        // enabled, and the kernel reports `enabled: false` simply because no
        // IP is currently assigned, which must not strip the desired config.
        let mut full_ipv4 = for_save.ipv4.clone();
        if saved.is_none()
            && let Some(ipv4) = full_ipv4.as_mut()
            && ipv4.addresses.is_none()
            && let Some(cur_ipv4) = current.and_then(|c| c.ipv4.as_ref())
        {
            ipv4.addresses = cur_ipv4.addresses.clone();
        }
        let mut full_ipv6 = for_save.ipv6.clone();
        if saved.is_none()
            && let Some(ipv6) = full_ipv6.as_mut()
            && ipv6.addresses.is_none()
            && let Some(cur_ipv6) = current.and_then(|c| c.ipv6.as_ref())
        {
            ipv6.addresses = cur_ipv6.addresses.clone();
        }

        let new_ipv4 = if let Some(des_ipv4) = desired.ipv4.as_ref() {
            let mut full_ipv4 = full_ipv4;
            let mut transition = false;
            if let Some(prev_ipv4) = prev_iface.and_then(|i| i.ipv4.as_ref()) {
                // Previous static → desired auto: discard the previous
                // static addresses. Only fire when the previous state really
                // holds static addresses — a disabled IPv4 stack
                // (`enabled: false`) must not trigger this, otherwise the
                // DHCP lease address would be discarded when the DHCP worker
                // applies a lease to an interface with no IPv4 yet.
                if des_ipv4.dhcp == Some(true) && prev_ipv4.is_static() {
                    if let Some(ipv4) = full_ipv4.as_mut() {
                        ipv4.addresses = None;
                    }
                    transition = true;
                }
                // Previous auto → desired static with no addresses
                // specified: IPv4 gets no IP.
                if prev_ipv4.is_auto()
                    && des_ipv4.dhcp == Some(false)
                    && des_ipv4.addresses.is_none()
                {
                    if let Some(ipv4) = full_ipv4.as_mut() {
                        ipv4.addresses = None;
                    }
                    transition = true;
                }
            }
            if ipv4_changed || transition {
                full_ipv4
            } else {
                None
            }
        } else {
            None
        };

        let new_ipv6 = if let Some(des_ipv6) = desired.ipv6.as_ref() {
            let mut full_ipv6 = full_ipv6;
            let mut transition = false;
            if let Some(prev_ipv6) = prev_iface.and_then(|i| i.ipv6.as_ref()) {
                // Previous static → desired auto: discard the previous
                // static addresses. Only fire when the previous state really
                // holds static addresses — see the IPv4 case above.
                if (des_ipv6.dhcp == Some(true)
                    || des_ipv6.autoconf == Some(true))
                    && prev_ipv6.is_static()
                {
                    if let Some(ipv6) = full_ipv6.as_mut() {
                        ipv6.addresses = None;
                    }
                    transition = true;
                }
                // Previous auto → desired static (`dhcp: false` +
                // `autoconf: false`) with no addresses specified: IPv6
                // keeps only the kernel-generated link-local address.
                if prev_ipv6.is_auto()
                    && des_ipv6.dhcp == Some(false)
                    && des_ipv6.autoconf == Some(false)
                    && des_ipv6.addresses.is_none()
                {
                    if let Some(ipv6) = full_ipv6.as_mut() {
                        ipv6.addresses = None;
                    }
                    transition = true;
                }
            }
            if ipv6_changed || transition {
                full_ipv6
            } else {
                None
            }
        } else {
            None
        };

        if let Some(ipv4) = new_ipv4 {
            for_apply.ipv4 = Some(ipv4.clone());
            if saved.is_some() {
                for_save.ipv4 = Some(ipv4);
            }
        }
        if let Some(ipv6) = new_ipv6 {
            for_apply.ipv6 = Some(ipv6.clone());
            if saved.is_some() {
                for_save.ipv6 = Some(ipv6);
            }
        }
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
        self.description = None;
        current.description = None;
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
        self.alt_name_pre_verify(current);
    }

    /// Adjust the current alt-names to match the desired list so the
    /// verification (an order-sensitive list comparison) passes.  Mirrors
    /// nmstate's `Interface::alt_name_pre_verify()`: `state: absent` entries
    /// are mirrored into the current list, and current alt-names not listed
    /// in the desired state are ignored (the apply only touches the listed
    /// entries, other alt-names such as systemd's auto-generated ones are
    /// left alone).
    fn alt_name_pre_verify(&self, current: &mut Self) {
        if let Some(des_alt_names) = self.alt_names.as_ref() {
            let cur_alt_names = current.alt_names.get_or_insert(Vec::new());
            for des_alt_name in des_alt_names {
                if des_alt_name.is_absent()
                    && !cur_alt_names
                        .iter()
                        .any(|c| c.name == des_alt_name.name)
                {
                    // Add the `state: absent` entry so the follow-up
                    // verification can pass.
                    cur_alt_names.push(des_alt_name.clone());
                }
            }
            cur_alt_names
                .retain(|cur_alt_name| des_alt_names.contains(cur_alt_name));
            cur_alt_names.sort_unstable();
        }
    }

    /// Sort the alternative names in place so the verification (which
    /// compares the list in order) matches the kernel-reported order after
    /// apply.
    fn sanitize_alt_names(&mut self) {
        if let Some(alt_names) = self.alt_names.as_mut() {
            alt_names.sort_unstable();
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
