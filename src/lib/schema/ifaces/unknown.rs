// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::{BaseInterface, JsonDisplay, NipartError, NipartInterface};

#[derive(
    Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonDisplay,
)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
/// Holder for interface with unknown interface type defined.
/// During apply action, nipart can resolve unknown interface to first
/// found interface type.
pub struct UnknownInterface {
    #[serde(flatten)]
    pub base: BaseInterface,
}

impl UnknownInterface {
    pub fn new(base: BaseInterface) -> Self {
        Self {
            base,
            ..Default::default()
        }
    }
}

impl NipartInterface for UnknownInterface {
    fn base_iface(&self) -> &BaseInterface {
        &self.base
    }

    fn base_iface_mut(&mut self) -> &mut BaseInterface {
        &mut self.base
    }

    /// Not sure is physical or kernel virtual interface, treat as virtual
    /// always.
    fn is_virtual(&self) -> bool {
        true
    }

    fn sanitize(
        &self,
        _current: Option<&Self>,
        _for_save: &mut Self,
        _for_apply: &mut Self,
        _for_verify: &mut Self,
        _merged: &mut Self,
    ) -> Result<(), NipartError> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "../unit_tests/ifaces_unknown.rs"]
mod tests;
