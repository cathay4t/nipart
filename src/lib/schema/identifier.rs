// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[non_exhaustive]
/// Interface Identifier defines the method for matching network interface.
pub enum InterfaceIdentifier {
    /// Use interface name to match the network interface, default value.
    #[default]
    Name,
    /// Use interface MAC address to match the network interface.
    /// Deserialize and serialize from/to `mac-address`.
    MacAddress,
}
