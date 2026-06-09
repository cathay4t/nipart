// SPDX-License-Identifier: Apache-2.0

use crate::{Interface, MergedInterface, NipartInterface};

impl MergedInterface {
    /// Removing loopback is treated as reset to default
    pub(crate) fn post_merge_sanitize_loopback(&mut self) {
        if self.merged.is_absent() {
            let mut lo = Interface::Loopback(Box::default());
            lo.base_iface_mut().kernel_iface_name = String::from("lo");
            self.for_apply = Some(lo.clone());
            self.for_verify = Some(lo);
        }
    }
}
