// SPDX-License-Identifier: Apache-2.0

use super::super::value::{copy_undefined_value, gen_diff_json_value};
use crate::{Interface, InterfaceState, NipartError, NipartInterface};

impl Interface {
    /// Generate a diff between self and old, returning a new Interface that
    /// only contains the differences, return None if identical
    pub fn gen_diff(&self, old: &Self) -> Result<Option<Self>, NipartError> {
        if self.is_absent() {
            let mut ret = old.clone();
            ret.base_iface_mut().state = InterfaceState::Absent;
            Ok(Some(ret))
        } else {
            let des_value = serde_json::to_value(self)?;
            let old_value = serde_json::to_value(old)?;
            if let Some(diff_value) =
                gen_diff_json_value(&des_value, &old_value)
            {
                let mut new_iface = self.clone_name_type_only();
                new_iface.base_iface_mut().state = self.base_iface().state;
                let mut new_iface_value = serde_json::to_value(&new_iface)?;
                copy_undefined_value(&mut new_iface_value, &diff_value);
                let mut new_iface =
                    serde_json::from_value::<Self>(new_iface_value)?;
                Self::include_diff_context(&mut new_iface, self, old);
                Ok(Some(new_iface))
            } else {
                Ok(None)
            }
        }
    }

    /// Generate a diff for the apply action. Same as [Interface::gen_diff],
    /// but the save-only `description` property is ignored so a
    /// description-only change does not trigger a kernel apply.
    pub(crate) fn gen_diff_for_apply(
        &self,
        old: &Self,
    ) -> Result<Option<Self>, NipartError> {
        let mut desired = self.clone();
        desired.base_iface_mut().description = None;
        let mut current = old.clone();
        current.base_iface_mut().description = None;
        desired.gen_diff(&current)
    }
}
