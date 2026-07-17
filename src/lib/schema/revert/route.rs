// SPDX-License-Identifier: Apache-2.0

use crate::{MergedRoutes, NipartError, RouteState, Routes};

impl MergedRoutes {
    pub(crate) fn generate_revert(&self) -> Result<Routes, NipartError> {
        let mut revert_routes = Vec::new();

        for rt in &self.changed_routes {
            if rt.is_absent() {
                for cur_rt in
                    self.current.config.as_ref().iter().flat_map(|r| r.iter())
                {
                    if rt.is_match(cur_rt) {
                        revert_routes.push(cur_rt.clone());
                    }
                }
            } else {
                let mut rt = rt.clone();
                rt.state = Some(RouteState::Absent);
                revert_routes.push(rt);
            }
        }

        Ok(Routes {
            running: None,
            config: if revert_routes.is_empty() {
                None
            } else {
                Some(revert_routes)
            },
        })
    }
}
