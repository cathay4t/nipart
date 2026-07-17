// SPDX-License-Identifier: Apache-2.0

use crate::{Interfaces, MergedInterfaces, NipartError};

impl MergedInterfaces {
    pub fn gen_diff(&self) -> Result<Interfaces, NipartError> {
        let mut ret = Interfaces::default();
        for merged_iface in self.iter() {
            if let Some(for_apply) = merged_iface.for_apply.as_ref() {
                if let Some(current) = merged_iface.current.as_ref() {
                    if let Some(diff) = for_apply.gen_diff(current)? {
                        ret.push(diff);
                    }
                } else {
                    ret.push(for_apply.clone())
                }
            }
        }
        Ok(ret)
    }
}
