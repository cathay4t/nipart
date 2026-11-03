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

pub(crate) fn vrf_link_builder(
    iface: &crate::VrfInterface,
    cur_iface: Option<&crate::VrfInterface>,
) -> Result<rtnetlink::LinkMessageBuilder<rtnetlink::LinkVrf>, crate::NipartError>
{
    use rtnetlink::packet_route::link::InfoKind;

    let mut builder =
        rtnetlink::LinkMessageBuilder::<rtnetlink::LinkVrf>::new_with_info_kind(
            InfoKind::Vrf,
        )
        .name(iface.base.name.to_string());

    let Some(table_id) = iface.vrf.as_ref().and_then(|c| c.table_id) else {
        if cur_iface.is_none() {
            return Err(crate::NipartError::new(
                crate::ErrorKind::InvalidArgument,
                format!(
                    "Need table ID for creating new VRF {}",
                    iface.base.name
                ),
            ));
        }
        return Ok(builder);
    };
    if table_id == 0 {
        return Err(crate::NipartError::new(
            crate::ErrorKind::InvalidArgument,
            format!("Invalid VRF table ID 0 for {}", iface.base.name),
        ));
    }
    if let Some(cur_iface) = cur_iface {
        if cur_iface.vrf.as_ref().and_then(|v| v.table_id) != Some(table_id) {
            return Err(crate::NipartError::new(
                crate::ErrorKind::InvalidArgument,
                format!(
                    "VRF table ID of {} cannot be changed, please delete and \
                     recreate the VRF interface",
                    iface.base.name
                ),
            ));
        }
    } else {
        builder = builder.table_id(table_id);
    }
    Ok(builder)
}
