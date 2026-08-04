// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nmstate project(https://nmstate.io/) which
// is under license of Apache 2.0, authors of original file are:
//  * Gris Ge <fge@redhat.com>
//  * Fernando Fernandez Mancera <ffmancera@riseup.net>
//  * Ales Musil <amusil@redhat.com>
//  * Jan Vaclav <jvaclav@redhat.com>
//  * Dan Kenigsberg <danken@redhat.com>
//  * Enrique Llorente <ellorent@redhat.com>
//  * Jan Vaclav <jvaclav@redhat.com>
//  * Rahul Rajesh <rajeshrah22@gmail.com>

use std::collections::HashMap;

use serde::{
    Deserialize, Deserializer, Serialize, Serializer, ser::SerializeSeq,
};

use crate::{
    ErrorKind, Interface, InterfaceIdentifier, InterfaceType,
    JsonDisplayHideSecrets, NipartError, NipartInterface, schema::IfaceSearch,
};

/// Represent a list of [Interface].
///
/// With special [serde::Deserializer] and [serde::Serializer].  When applying
/// complex nested interface(e.g. bridge over bond over vlan of eth1), the
/// supported maximum nest level is 4.  For 5+ nested
/// level, you need to place controller interface before its ports.
#[derive(Clone, Debug, Default, PartialEq, Eq, JsonDisplayHideSecrets)]
#[non_exhaustive]
pub struct Interfaces {
    /// Holding all interfaces with kernel representative. E.g. ethernet, bond.
    pub kernel_ifaces: HashMap<String, Interface>,
    /// Holding all interfaces which only exist in user space tool.
    /// For example: OVS bridge and IPSec
    pub user_ifaces: HashMap<(String, InterfaceType), Interface>,
    // The insert_order is allowing user to provided ordered interface
    // to support 5+ nested dependency.
    pub(crate) insert_order: Vec<(String, InterfaceType)>,
}

impl<'de> Deserialize<'de> for Interfaces {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut ret = Self::default();

        for iface in <Vec<Interface> as Deserialize>::deserialize(deserializer)?
        {
            ret.push(iface);
        }
        Ok(ret)
    }
}

impl Serialize for Interfaces {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let ifaces = self.to_vec();
        let mut seq = serializer.serialize_seq(Some(ifaces.len()))?;
        for iface in ifaces {
            seq.serialize_element(iface)?;
        }
        seq.end()
    }
}

impl Interfaces {
    pub fn is_empty(&self) -> bool {
        self.kernel_ifaces.is_empty() && self.user_ifaces.is_empty()
    }

    /// Extract internal interfaces to `Vec()` and sorted by `up_priority`
    pub fn to_vec(&self) -> Vec<&Interface> {
        let mut ifaces = Vec::new();
        for iface in self.kernel_ifaces.values() {
            ifaces.push(iface);
        }
        for iface in self.user_ifaces.values() {
            ifaces.push(iface);
        }
        ifaces.sort_unstable_by_key(|iface| {
            (
                iface.base_iface().iface_index.unwrap_or(u32::MAX),
                iface.name(),
            )
        });
        // Use sort_by_key() instead of unstable one, do we can alphabet
        // activation order which is required to simulate the OS boot-up.
        ifaces.sort_by_key(|iface| iface.base_iface().up_priority);

        ifaces
    }

    pub(crate) fn hide_secrets(&mut self) {
        for iface in self.iter_mut() {
            iface.base_iface_mut().hide_secrets();
            iface.hide_secrets();
        }
    }

    /// The iteration order is not sorted by `up_priority`
    pub fn iter(&self) -> impl Iterator<Item = &Interface> {
        self.kernel_ifaces.values().chain(self.user_ifaces.values())
    }

    /// The iteration order is not sorted by `up_priority`
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Interface> {
        self.kernel_ifaces
            .values_mut()
            .chain(self.user_ifaces.values_mut())
    }

    /// The iteration order is not sorted by `up_priority`
    pub fn drain(&mut self) -> impl Iterator<Item = Interface> {
        self.kernel_ifaces
            .drain()
            .map(|(_, iface)| iface)
            .chain(self.user_ifaces.drain().map(|(_, iface)| iface))
    }

    /// Search interface based on interface name and interface type.
    /// When iface_type is defined, we validate interface type also.
    /// The first interface matches will be returned.
    pub fn get<'a>(
        &'a self,
        kernel_iface_name: &str,
        iface_type: Option<&InterfaceType>,
    ) -> Option<&'a Interface> {
        for iface in self
            .iter()
            .filter(|i| i.kernel_iface_name() == kernel_iface_name)
        {
            if let Some(des_iface_type) = iface_type {
                if des_iface_type == iface.iface_type() {
                    return Some(iface);
                }
            } else {
                return Some(iface);
            }
        }
        None
    }

    pub fn get_mut<'a>(
        &'a mut self,
        kernel_iface_name: &str,
        iface_type: Option<&InterfaceType>,
    ) -> Option<&'a mut Interface> {
        for iface in self
            .iter_mut()
            .filter(|i| i.kernel_iface_name() == kernel_iface_name)
        {
            if let Some(des_iface_type) = iface_type {
                if des_iface_type == iface.iface_type() {
                    return Some(iface);
                }
            } else {
                return Some(iface);
            }
        }
        None
    }

    /// Append specified [Interface].
    pub fn push(&mut self, mut iface: Interface) {
        if iface.kernel_iface_name().is_empty() {
            iface.base_iface_mut().kernel_iface_name = iface.name().to_string();
        }
        let iface_key = iface.kernel_iface_name().to_string();
        self.insert_order
            .push((iface_key.clone(), iface.iface_type().clone()));
        if iface.is_userspace() {
            self.user_ifaces
                .insert((iface_key, iface.iface_type().clone()), iface);
        } else {
            self.kernel_ifaces.insert(iface_key, iface);
        }
    }

    /// For all interfaces without `InterfaceIdentifier::MacAddress`,
    /// ensure `kernel_iface_name` is copied from `name()`.
    /// If user provided `kernel_iface_name` in YAML, logs an INFO message
    /// that it is ignored for apply and overwrites it.
    pub fn resolve_name_identifier(&mut self) {
        for iface in self.iter_mut() {
            let base = iface.base_iface_mut();
            if base.identifier == Some(InterfaceIdentifier::MacAddress) {
                continue;
            }
            base.kernel_iface_name.clone_from(&base.name);
        }
    }

    /// Resolve desired interfaces with `identifier: mac-address` by matching
    /// against current kernel interfaces by MAC address.
    /// Renames the interface to the kernel interface name for backward compat
    /// with merge logic, stores logical name as `profile_name`, and sets
    /// `kernel_iface_name` to the kernel interface name. Resolves unknown type.
    pub fn resolve_mac_identifier(
        &mut self,
        current: &Self,
    ) -> Result<(), NipartError> {
        let search = IfaceSearch::new(&Interfaces::default(), current);
        let mut changed_ifaces: Vec<(String, Interface)> = Vec::new();
        for iface in self.iter().filter(|i| {
            (!i.is_absent())
                && i.base_iface().identifier
                    == Some(InterfaceIdentifier::MacAddress)
                && (i.iface_type() == &InterfaceType::Ethernet
                    || i.iface_type().is_unknown())
        }) {
            let Some(mac_address) = iface.base_iface().mac_address.as_deref()
            else {
                if iface.base_iface().profile_name.is_some()
                    && !iface.base_iface().kernel_iface_name.is_empty()
                {
                    continue;
                }
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "Desired interface {} has `identifier: mac-address` \
                         but MAC address is not defined",
                        iface.name()
                    ),
                ));
            };
            let logical_name = iface.base_iface().name.clone();
            let kernel_name = search
                .search_iface(InterfaceIdentifier::MacAddress, mac_address)?;
            let kernel_name = kernel_name.to_string();

            if iface.base_iface().name == kernel_name
                && iface.base_iface().profile_name.is_some()
                && !iface.base_iface().kernel_iface_name.is_empty()
            {
                continue;
            }

            let cur_iface = current.kernel_ifaces.get(&kernel_name).unwrap();

            let mut new_iface = if iface.iface_type().is_unknown() {
                let mut new_iface_value = serde_json::to_value(iface)?;
                if let Some(obj) = new_iface_value.as_object_mut() {
                    obj.insert(
                        "type".to_string(),
                        serde_json::Value::String(
                            cur_iface.iface_type().to_string(),
                        ),
                    );
                }
                Interface::deserialize(new_iface_value)?
            } else {
                iface.clone()
            };
            if new_iface.base_iface().profile_name.is_none() {
                new_iface.base_iface_mut().profile_name =
                    Some(logical_name.clone());
            }
            new_iface.base_iface_mut().name = kernel_name.clone();
            new_iface.base_iface_mut().kernel_iface_name = kernel_name;
            changed_ifaces.push((logical_name.clone(), new_iface));
        }
        for (old_name, new_iface) in changed_ifaces {
            self.kernel_ifaces.remove(&old_name);
            self.user_ifaces.retain(|(n, _), _| n != &old_name);
            self.push(new_iface);
        }
        Ok(())
    }

    /// Resolve desired interfaces with `identifier: mac-address` by matching
    /// against current kernel interfaces by MAC address.
    /// Updates the desired interface name to the matched kernel interface name,
    /// stores original name as `profile_name`, and resolves unknown type.
    pub fn resolve_mac_identifier_in_desired(
        &mut self,
        current: &Self,
    ) -> Result<(), NipartError> {
        let search = IfaceSearch::new(&Interfaces::default(), current);
        let mut changed_ifaces: Vec<Interface> = Vec::new();
        for iface in self.iter().filter(|i| {
            (!i.is_absent())
                && i.base_iface().identifier
                    == Some(InterfaceIdentifier::MacAddress)
                && i.base_iface().profile_name.is_none()
        }) {
            let mac_address = match iface.base_iface().mac_address.as_deref() {
                Some(m) => m,
                None => {
                    return Err(NipartError::new(
                        ErrorKind::InvalidArgument,
                        format!(
                            "Desired interface {} has `identifier: \
                             mac-address` but not MAC address defined",
                            iface.name()
                        ),
                    ));
                }
            };
            let kernel_name = search
                .search_iface(InterfaceIdentifier::MacAddress, mac_address)?;
            let kernel_name = kernel_name.to_string();
            let cur_iface = current.kernel_ifaces.get(&kernel_name).unwrap();

            let mut new_iface = if iface.iface_type().is_unknown() {
                let mut new_iface_value = serde_json::to_value(iface)?;
                if let Some(obj) = new_iface_value.as_object_mut() {
                    obj.insert(
                        "type".to_string(),
                        serde_json::Value::String(
                            cur_iface.iface_type().to_string(),
                        ),
                    );
                }
                Interface::deserialize(new_iface_value)?
            } else {
                iface.clone()
            };
            new_iface.base_iface_mut().profile_name =
                Some(iface.base_iface().name.clone());
            new_iface.base_iface_mut().name = kernel_name;
            changed_ifaces.push(new_iface);
        }
        for changed_iface in changed_ifaces {
            if let Some(profile_name) =
                changed_iface.base_iface().profile_name.as_deref()
            {
                self.kernel_ifaces.remove(profile_name);
            }
            self.push(changed_iface);
        }
        Ok(())
    }

    /// Remove interface based on interface name and interface type.
    /// When iface_type is defined, we validate interface type also.
    /// The first interface matches will be returned.
    /// If iface_type is undefined, only search kernel interfaces
    pub fn remove(
        &mut self,
        kernel_iface_name: &str,
        iface_type: Option<&InterfaceType>,
    ) -> Option<Interface> {
        if let Some(iface_type) = iface_type {
            if iface_type.is_userspace() {
                self.user_ifaces.remove(&(
                    kernel_iface_name.to_string(),
                    iface_type.clone(),
                ))
            } else if let Some(iface) =
                self.kernel_ifaces.get(kernel_iface_name)
            {
                if iface.iface_type() == iface_type {
                    self.kernel_ifaces.remove(kernel_iface_name)
                } else {
                    log::debug!(
                        "Interfaces::remove(): found interface \
                         {kernel_iface_name} holding {}, not requested \
                         {iface_type}",
                        iface.iface_type()
                    );
                    None
                }
            } else {
                None
            }
        } else {
            self.kernel_ifaces.remove(kernel_iface_name)
        }
    }
}
