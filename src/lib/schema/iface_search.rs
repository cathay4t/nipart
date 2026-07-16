// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};

use crate::{
    ErrorKind, InterfaceIdentifier, Interfaces, NipartError, NipartInterface,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct IfaceSearch {
    perm_mac_to_kernel: HashMap<String, Vec<String>>,
    mac_to_kernel: HashMap<String, Vec<String>>,
    profile_to_kernel: HashMap<String, Vec<String>>,
    kernel_names: HashSet<String>,
}

impl IfaceSearch {
    pub(crate) fn new(
        desired_ifaces: &Interfaces,
        current_ifaces: &Interfaces,
    ) -> Self {
        let mut ret = Self::default();
        // TODO: MergedInterface should generate a Interface for us to
        //       build search cache after process the `state: absent`.
        // BUG: we does not handle desired state changed current interface.
        for iface in desired_ifaces.iter().filter(|i| !i.is_absent()) {
            ret.collect_ethernet(iface);
        }
        for iface in current_ifaces.iter() {
            ret.collect_ethernet(iface);
        }
        ret
    }

    pub(crate) fn new_from_merged(
        merged_ifaces: &crate::MergedInterfaces,
    ) -> Self {
        let mut ret = Self::default();
        for merged_iface in merged_ifaces.iter() {
            ret.collect_ethernet(&merged_iface.merged);
        }
        ret
    }

    fn collect_ethernet(&mut self, iface: &crate::Interface) {
        if !matches!(iface.iface_type(), crate::InterfaceType::Ethernet) {
            return;
        }
        let base = iface.base_iface();
        let kernel_name = if base.kernel_iface_name.is_empty() {
            base.name.as_str()
        } else {
            base.kernel_iface_name.as_str()
        };

        self.kernel_names.insert(kernel_name.to_string());

        if let Some(mac) = base.mac_address.as_deref()
            && !mac.is_empty()
        {
            self.mac_to_kernel
                .entry(mac.to_ascii_uppercase())
                .or_default()
                .push(kernel_name.to_string());
        }
        if let Some(perm_mac) = base.permanent_mac_address.as_deref()
            && !perm_mac.is_empty()
        {
            self.perm_mac_to_kernel
                .entry(perm_mac.to_ascii_uppercase())
                .or_default()
                .push(kernel_name.to_string());
        }
        if let Some(profile) = base.profile_name.as_deref() {
            self.profile_to_kernel
                .entry(profile.to_string())
                .or_default()
                .push(kernel_name.to_string());
        }
    }

    fn lookup_single(
        map: &HashMap<String, Vec<String>>,
        key: &str,
    ) -> Result<String, NipartError> {
        match map.get(key) {
            Some(names) if names.len() == 1 => Ok(names[0].clone()),
            Some(names) => Err(NipartError::new(
                ErrorKind::InvalidArgument,
                format!(
                    "Multiple interfaces found for '{key}': {}",
                    names.join(", ")
                ),
            )),
            None => Err(NipartError::new(
                ErrorKind::InvalidArgument,
                format!("No interface found for '{key}'"),
            )),
        }
    }

    pub(crate) fn search_iface(
        &self,
        identifier: InterfaceIdentifier,
        val: &str,
    ) -> Result<String, NipartError> {
        match identifier {
            InterfaceIdentifier::MacAddress => {
                let val = val.to_ascii_uppercase();
                if let Ok(name) =
                    Self::lookup_single(&self.perm_mac_to_kernel, &val)
                {
                    return Ok(name);
                }
                Self::lookup_single(&self.mac_to_kernel, &val)
            }
            InterfaceIdentifier::Name => {
                if self.kernel_names.contains(val) {
                    return Ok(val.to_string());
                }
                Self::lookup_single(&self.profile_to_kernel, val)
            }
        }
    }

    pub(crate) fn resolve_kernel_iface_name(
        &self,
        name: &str,
    ) -> Result<String, NipartError> {
        if self.kernel_names.contains(name) {
            return Ok(name.to_string());
        }
        match self.profile_to_kernel.get(name) {
            Some(names) if names.len() == 1 => Ok(names[0].clone()),
            Some(names) => Err(NipartError::new(
                ErrorKind::InvalidArgument,
                format!(
                    "Route next-hop-interface '{name}' matches multiple \
                     interfaces by logical name, candidates: {}",
                    names.join(", ")
                ),
            )),
            None => Ok(name.to_string()),
        }
    }
}
