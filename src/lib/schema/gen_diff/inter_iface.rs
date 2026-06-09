// SPDX-License-Identifier: Apache-2.0

use crate::{Interfaces, MergedInterfaces, NipartError, NipartInterface};

impl MergedInterfaces {
    pub fn gen_diff(&self) -> Result<Interfaces, NipartError> {
        let mut ret = Interfaces::default();
        for diff_iface in self
            .kernel_ifaces
            .values()
            .chain(self.user_ifaces.values())
            .filter_map(|i| i.diff.as_ref())
        {
            ret.push(diff_iface.clone())
        }
        Ok(ret)
    }
}

impl Interfaces {
    pub(crate) fn sanitize_for_diff(&mut self, current: &mut Self) {
        for iface in self.iter_mut() {
            if let Some(cur_iface) = current
                .get_mut(iface.kernel_iface_name(), Some(iface.iface_type()))
            {
                iface.sanitize_for_diff(cur_iface);
            }
        }
    }
}
