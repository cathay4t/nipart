// SPDX-License-Identifier: Apache-2.0

use crate::BaseInterface;

impl BaseInterface {
    pub(crate) fn include_revert_context(
        &mut self,
        _desired: &Self,
        pre_apply: &Self,
    ) {
        if self.ipv4.is_some() {
            self.ipv4 = pre_apply.ipv4.clone();
        }
        if self.ipv6.is_some() {
            self.ipv6 = pre_apply.ipv6.clone();
        }
    }
}
