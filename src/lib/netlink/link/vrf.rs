// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nispor project
// (https://github.com/nispor/nispor) which is under license of Apache 2.0.

use rtnetlink::packet_route::link::{InfoData, InfoVrf};

use crate::VrfConfig;

pub(crate) fn parse_vrf(data: &InfoData) -> Option<VrfConfig> {
    let InfoData::Vrf(infos) = data else {
        return None;
    };
    let mut table_id = 0u32;
    for info in infos {
        if let InfoVrf::TableId(id) = info {
            table_id = *id;
        }
    }
    Some(VrfConfig {
        table_id: Some(table_id),
        ports: Some(Vec::new()),
    })
}
