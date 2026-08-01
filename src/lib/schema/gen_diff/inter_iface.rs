// SPDX-License-Identifier: Apache-2.0

use crate::{Interfaces, MergedInterfaces, NipartError, NipartInterface};

impl MergedInterfaces {
    pub fn gen_diff(&self) -> Result<Interfaces, NipartError> {
        let mut ret = Interfaces::default();

        for merged_iface in self.iter() {
            match (
                merged_iface.for_apply.as_ref(),
                merged_iface.current.as_ref(),
            ) {
                (Some(des_iface), Some(cur_iface)) => {
                    if let Some(diff_iface) = des_iface.gen_diff(cur_iface)? {
                        ret.push(diff_iface);
                    }
                }
                (Some(des_iface), None) => {
                    ret.push(des_iface.clone());
                }
                _ => (),
            }
        }
        Ok(ret)
    }
}

impl Interfaces {
    pub fn gen_diff(&self, old: &Self) -> Result<Interfaces, NipartError> {
        let mut ret = Interfaces::default();
        for new_iface in self.iter() {
            if let Some(cur_iface) = old.get(new_iface.base_iface()) {
                if let Some(diff_iface) = new_iface.gen_diff(cur_iface)? {
                    ret.push(diff_iface);
                }
            } else {
                ret.push(new_iface.clone());
            }
        }
        Ok(ret)
    }
}
