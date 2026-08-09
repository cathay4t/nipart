// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

/// Alternative name of the interface.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Default,
    Serialize,
    Deserialize,
)]
#[non_exhaustive]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct AltNameEntry {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Set to `state: absent` will delete specified alternative name.
    pub state: Option<AltNameState>,
}

/// The only supported state for an alternative name is `absent`.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Default,
    Serialize,
    Deserialize,
)]
#[non_exhaustive]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub enum AltNameState {
    #[default]
    Absent,
}

impl AltNameEntry {
    pub fn is_absent(&self) -> bool {
        self.state == Some(AltNameState::Absent)
    }
}
