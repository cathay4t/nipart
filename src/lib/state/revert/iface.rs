// SPDX-License-Identifier: Apache-2.0

use crate::{
    Interface, InterfaceState, MergedInterface, NipartError, NipartInterface,
};

use super::value::gen_revert_state;

impl MergedInterface {
    pub(crate) fn generate_revert(
        &self,
    ) -> Result<Option<Interface>, NipartError> {
        let apply_iface = match self.for_apply.as_ref() {
            Some(i) => i,
            None => return Ok(None),
        };
        if let Some(cur_iface) = self.current.as_ref() {
            let revert_iface = apply_iface.generate_revert(cur_iface)?;
            if !is_no_op(&revert_iface, apply_iface) {
                Ok(Some(revert_iface))
            } else {
                Ok(None)
            }
        } else {
            let mut revert_iface = apply_iface.clone_name_type_only();
            revert_iface.base_iface_mut().state = InterfaceState::Absent;
            Ok(Some(revert_iface))
        }
    }
}

impl Interface {
    pub(crate) fn generate_revert(
        &self,
        current: &Self,
    ) -> Result<Self, NipartError> {
        if self.is_absent() {
            return Ok(current.clone());
        }

        let mut revert_value =
            serde_json::to_value(current.clone_name_type_only())?;
        let desired_value = serde_json::to_value(self)?;
        let current_value = serde_json::to_value(current)?;

        gen_revert_state(&desired_value, &current_value, &mut revert_value);

        let mut revert_iface: Interface = serde_json::from_value(revert_value)?;

        revert_iface.include_revert_context(self, current);

        Ok(revert_iface)
    }
}

fn is_no_op(revert_iface: &Interface, desired_iface: &Interface) -> bool {
    if let (Ok(revert_value), Ok(desired_value)) = (
        serde_json::to_value(revert_iface),
        serde_json::to_value(desired_iface),
    ) {
        revert_value == desired_value
    } else {
        false
    }
}
