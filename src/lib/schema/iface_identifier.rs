// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::BaseInterface;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[non_exhaustive]
/// Interface Identifier defines the method for matching network interface.
pub enum InterfaceIdentifier {
    /// If `kernel_iface_name` property or (`kernel-iface-name` in JSON/YAML)
    /// defined, use that property to match network interface.
    /// If `kernel_iface_name` is undefined or empty, use `name` property.
    /// This is the default value if undefined.
    #[default]
    Name,
    /// Use interface MAC address to match the network interface, prefer
    /// persistent MAC address.
    /// Deserialize and serialize from/to `mac-address`.
    MacAddress,
}

impl BaseInterface {
    pub(crate) fn copy_identifier_config_from(&mut self, src: &Self) {
        let dst = self;
        dst.identifier = src.identifier;
        if src.identifier == Some(InterfaceIdentifier::MacAddress) {
            if src.mac_address.is_none() {
            } else {
                dst.mac_address = src.mac_address.clone();
            }
        }
    }
}
