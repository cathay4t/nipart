// SPDX-License-Identifier: Apache-2.0

use crate::{MergedNetworkState, NetworkState, NipartApplyOption, NipartError};

impl MergedNetworkState {
    pub fn gen_diff(&self) -> Result<NetworkState, NipartError> {
        Ok(NetworkState {
            version: self.version,
            description: self.description.clone(),
            ifaces: self.ifaces.gen_diff()?,
            routes: self.routes.gen_diff(),
            wait_online: self.desired.wait_online.clone(),
        })
    }
}

impl NetworkState {
    /// Generate NetworkState containing only the properties changed comparing
    /// to `old_state`.
    pub fn gen_diff(&self, old: &Self) -> Result<Self, NipartError> {
        let merged_state = MergedNetworkState::new(
            self.clone(),
            old.clone(),
            None,
            NipartApplyOption::default(),
        )?;
        merged_state.gen_diff()
    }
}
