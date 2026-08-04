// SPDX-License-Identifier: Apache-2.0

use super::value::gen_revert_state;
use crate::{
    Interface, InterfaceState, MergedInterface, NipartError, NipartInterface,
};

impl MergedInterface {
    pub(crate) fn generate_revert(
        &self,
    ) -> Result<Option<Interface>, NipartError> {
        let apply_iface = match self.for_apply.as_ref() {
            Some(i) => i,
            None => return Ok(None),
        };
        if self.current.is_some() {
            apply_iface.generate_revert(self.current.as_ref())
        } else {
            let mut revert_iface = apply_iface.clone_name_type_only();
            revert_iface.base_iface_mut().state = InterfaceState::Absent;
            Ok(Some(revert_iface))
        }
    }
}

impl Interface {
    /// Return revert state applied by self with pre-apply current.
    /// Return None if nothing changed.
    pub(crate) fn generate_revert(
        &self,
        current: Option<&Self>,
    ) -> Result<Option<Self>, NipartError> {
        let for_apply = self;
        if for_apply.is_absent() {
            return Ok(current.cloned());
        }
        let Some(current) = current.as_ref() else {
            let mut ret = for_apply.clone();
            ret.base_iface_mut().state = InterfaceState::Absent;
            return Ok(Some(ret));
        };

        let mut revert_value =
            serde_json::to_value(current.clone_name_type_only())?;
        let desired_value = serde_json::to_value(for_apply)?;
        let current_value = serde_json::to_value(current)?;

        gen_revert_state(&desired_value, &current_value, &mut revert_value);

        let mut revert_iface: Interface = serde_json::from_value(revert_value)?;

        revert_iface.base_iface_mut().include_revert_context(
            for_apply.base_iface(),
            current.base_iface(),
        );
        revert_iface.include_revert_context(for_apply, current);

        if is_no_op(&revert_iface, for_apply) {
            Ok(None)
        } else {
            Ok(Some(revert_iface))
        }
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
