// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::{BaseInterface, InterfaceState, InterfaceType, NipartError};

/// Trait implemented by all type of interfaces.
pub trait NipartInterface:
    std::fmt::Debug + for<'a> Deserialize<'a> + Serialize + Default + Clone
{
    fn base_iface(&self) -> &BaseInterface;

    fn base_iface_mut(&mut self) -> &mut BaseInterface;

    /// Whether interface is physical interface or create by kernel or
    /// userspace at runtime.
    fn is_virtual(&self) -> bool;

    /// Whether specified interface only exist in user space configuration
    /// without any kernel interface index.
    fn is_userspace(&self) -> bool {
        self.iface_type().is_userspace()
    }

    /// Whether can hold ports
    fn is_controller(&self) -> bool {
        self.iface_type().is_controller()
    }

    /// Whether attached to controller
    fn has_controller(&self) -> bool {
        if let Some(ctrl) = self.base_iface().controller.as_ref()
            && !ctrl.is_empty()
        {
            true
        } else {
            false
        }
    }

    fn has_parent(&self) -> bool {
        if let Some(parent) = self.parent()
            && !parent.is_empty()
        {
            true
        } else {
            false
        }
    }

    fn name(&self) -> &str {
        self.base_iface().name.as_str()
    }

    fn is_name_matching(&self) -> bool {
        self.base_iface().is_name_matching()
    }

    /// The actual kernel interface name.
    /// When not resolved to real kernel interface for
    /// [InterfaceIdentifier::MacAddress], this is empty string.
    /// For user space interface, also return empty string.
    fn kernel_iface_name(&self) -> &str {
        if self.base_iface().kernel_iface_name.is_empty()
            && self.is_name_matching()
        {
            self.base_iface().name.as_str()
        } else {
            self.base_iface().kernel_iface_name.as_str()
        }
    }

    fn iface_type(&self) -> &InterfaceType {
        &self.base_iface().iface_type
    }

    fn iface_state(&self) -> InterfaceState {
        self.base_iface().state
    }

    /// Replace the secrets with `NetworkState::HIDE_SECRET_STR`
    fn hide_secrets(&mut self) {}

    fn is_ignore(&self) -> bool {
        self.base_iface().state.is_ignore()
    }

    fn is_up(&self) -> bool {
        self.base_iface().state.is_up()
    }

    fn is_down(&self) -> bool {
        self.base_iface().state.is_down()
    }

    fn is_absent(&self) -> bool {
        self.base_iface().state.is_absent()
    }

    /// Please implemented this function if special merge action required
    /// for certain interface type.
    /// The self is the merged state.
    /// This function will not invoke BaseInterface.post_merge(), callers should
    /// invoke it by themselves.
    fn post_merge(
        &mut self,
        _new_state: &Self,
        _old_state: &Self,
    ) -> Result<(), NipartError> {
        Ok(())
    }

    /// Sanitation process is performed for apply action:
    ///  * Validate user inputs.
    ///  * Clean up properties which is for query only.
    ///  * Change desired state smartly. (e.g. Change MAC address to upper case)
    ///
    /// The self is the desired state.
    /// This function will not invoke BaseInterface.sanitize(), callers should
    /// invoke it by themselves.
    fn sanitize(
        &self,
        current: Option<&Self>,
        for_save: &mut Self,
        for_apply: &mut Self,
        for_verify: &mut Self,
        merged: &mut Self,
    ) -> Result<(), NipartError>;

    /// Invoke sanitize current for verify desired state.
    /// The self is the desired state.
    /// This function will not invoke BaseInterface.sanitize_before_verify(),
    /// callers should invoke it by themselves.
    fn sanitize_before_verify(&mut self, _current: &mut Self) {}

    /// When generating difference between desired and current, certain value
    /// should be included as context in the output. For example, when
    /// VLAN ID changed, including base-iface as context seems reasonable.
    /// Default implementation does nothing.
    /// The self is the generated diff state.
    /// This function will not invoke BaseInterface.include_diff_context(),
    /// callers should invoke it by themselves.
    fn include_diff_context(&mut self, _desired: &Self, _current: &Self) {}

    fn from_base(base_iface: BaseInterface) -> Self {
        let mut new = Self::default();
        *new.base_iface_mut() = base_iface;
        new
    }

    /// When generating revert state for desired state, certain value
    /// should be included as context in the output. For example, when
    /// reverting a IP disable action, we should include its original static
    /// IP addresses.
    /// The self is the revert state.
    /// This function will not invoke BaseInterface.include_revert_context(),
    /// callers should invoke it by themselves.
    fn include_revert_context(&mut self, _desired: &Self, _pre_apply: &Self) {}

    /// Return a list of port names. None means not desired or cannot hold
    /// ports
    fn ports(&self) -> Option<Vec<&str>> {
        None
    }

    /// Return parent interface name, None means not desired or no parent
    fn parent(&self) -> Option<&str> {
        None
    }

    /// Whether desired changes need to delete the interface first.
    fn need_delete_before_change(&self, _current: &Self) -> bool {
        false
    }
}
