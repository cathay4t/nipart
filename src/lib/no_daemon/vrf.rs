// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nmstate project(https://nmstate.io/) which
// is under license of Apache 2.0, authors of original file
// (rust/src/lib/nispor/vrf.rs) are:
//  * Gris Ge <fge@redhat.com>

use crate::{BaseInterface, VrfConfig, VrfInterface};

impl VrfInterface {
    pub(crate) fn new_from_nispor(
        base_iface: BaseInterface,
        np_iface: &nispor::Iface,
    ) -> Self {
        let vrf_conf = np_iface.vrf.as_ref().map(|np_vrf_info| VrfConfig {
            table_id: Some(np_vrf_info.table_id),
            ports: {
                let mut ports = np_vrf_info.ports.clone();
                ports.sort_unstable();
                Some(ports)
            },
        });

        Self {
            base: base_iface,
            vrf: vrf_conf,
        }
    }
}
