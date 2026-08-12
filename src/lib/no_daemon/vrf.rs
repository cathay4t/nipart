// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nmstate project(https://nmstate.io/) which
// is under license of Apache 2.0, authors of original file
// (rust/src/lib/nispor/vrf.rs) are:
//  * Gris Ge <fge@redhat.com>

use futures_util::stream::TryStreamExt;

use crate::{
    BaseInterface, ErrorKind, Interface, InterfaceType, MergedInterface,
    NipartError, NipartInterface, VrfConfig, VrfInterface,
};

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

/// Apply VRF link level changes via rtnetlink directly as nispor does not
/// support creating VRF interface:
///  * Create the VRF interface when it does not exist.
///  * Linux kernel does not support changing VRF route table ID on an existing
///    interface, hence when table ID changed, we delete and recreate the VRF
///    interface, then re-attach all its ports (deleting the master
///    automatically detaches the enslaved ports).
///
/// The port attach/detach of a VRF without table ID change is handled by the
/// generic controller mechanism through nispor.
pub(crate) async fn apply_vrf_link_changes(
    merged_iface: &MergedInterface,
) -> Result<(), NipartError> {
    let Some(apply_iface) = merged_iface.for_apply.as_ref() else {
        return Ok(());
    };
    let Interface::Vrf(vrf_iface) = apply_iface else {
        return Ok(());
    };
    let Some(des_table_id) = vrf_iface.vrf.as_ref().and_then(|v| v.table_id)
    else {
        // No table ID in desired diff: nothing to do here, base changes
        // (MTU/state/alt-name) are handled by the nispor batch.
        return Ok(());
    };

    let cur_iface = merged_iface.current.as_ref();
    let cur_table_id = cur_iface.and_then(|c| match c {
        Interface::Vrf(c) => c.vrf.as_ref().and_then(|v| v.table_id),
        _ => None,
    });

    let (conn, handle, _) = rtnetlink::new_connection().map_err(|e| {
        NipartError::new(
            ErrorKind::Bug,
            format!("Failed to create rtnetlink connection: {e}"),
        )
    })?;
    tokio::spawn(conn);

    if cur_iface.is_none() {
        // New VRF interface, create it.
        create_vrf(&handle, merged_iface, vrf_iface, des_table_id).await?;
    } else if cur_table_id != Some(des_table_id) {
        // Table ID changed: delete and recreate, then re-attach ports.
        log::info!(
            "VRF interface {} route table ID changed from {:?} to {}, \
             deleting and recreating",
            merged_iface.merged.name(),
            cur_table_id,
            des_table_id,
        );
        let cur_index = cur_iface
            .and_then(|c| c.base_iface().iface_index)
            .ok_or_else(|| {
                NipartError::new(
                    ErrorKind::Bug,
                    format!(
                        "Current VRF interface {} holding no iface_index",
                        merged_iface.merged.name()
                    ),
                )
            })?;
        handle.link().del(cur_index).execute().await.map_err(|e| {
            NipartError::new(
                ErrorKind::Bug,
                format!(
                    "Failed to delete VRF interface {}: {e}",
                    merged_iface.merged.name()
                ),
            )
        })?;
        create_vrf(&handle, merged_iface, vrf_iface, des_table_id).await?;
        // Deleting the VRF automatically detaches all enslaved ports,
        // re-attach the desired port list.
        let ports: Vec<String> = merged_iface
            .merged
            .ports()
            .unwrap_or_default()
            .iter()
            .map(|p| p.to_string())
            .collect();
        if !ports.is_empty() {
            let vrf_index = resolve_iface_index(
                &handle,
                merged_iface.merged.kernel_iface_name(),
            )
            .await?;
            for port_name in ports {
                let port_index =
                    resolve_iface_index(&handle, &port_name).await?;
                log::debug!(
                    "Attaching port {port_name} to VRF interface {}",
                    merged_iface.merged.name()
                );
                let msg = rtnetlink::LinkUnspec::new_with_index(port_index)
                    .controller(vrf_index)
                    .build();
                handle.link().set(msg).execute().await.map_err(|e| {
                    NipartError::new(
                        ErrorKind::Bug,
                        format!(
                            "Failed to attach port {port_name} to VRF \
                             interface {}: {e}",
                            merged_iface.merged.name()
                        ),
                    )
                })?;
            }
        }
    }

    Ok(())
}

async fn create_vrf(
    handle: &rtnetlink::Handle,
    merged_iface: &MergedInterface,
    vrf_iface: &VrfInterface,
    table_id: u32,
) -> Result<(), NipartError> {
    let mut builder = rtnetlink::LinkVrf::new(
        merged_iface.merged.kernel_iface_name(),
        table_id,
    );
    if let Some(mtu) = vrf_iface.base.mtu {
        builder = builder.mtu(mtu as u32);
    }
    if vrf_iface.is_up() {
        builder = builder.up();
    }
    log::info!(
        "Creating VRF interface {}/{} with route table ID {table_id}",
        merged_iface.merged.name(),
        InterfaceType::Vrf,
    );
    let msg = builder.build();
    handle.link().add(msg).execute().await.map_err(|e| {
        NipartError::new(
            ErrorKind::Bug,
            format!(
                "Failed to create VRF interface {}: {e}",
                merged_iface.merged.name()
            ),
        )
    })
}

async fn resolve_iface_index(
    handle: &rtnetlink::Handle,
    iface_name: &str,
) -> Result<u32, NipartError> {
    let mut links = handle
        .link()
        .get()
        .match_name(iface_name.to_string())
        .execute();
    while let Some(nl_msg) = links.try_next().await.map_err(|e| {
        NipartError::new(
            ErrorKind::Bug,
            format!("Failed to query interface {iface_name}: {e}"),
        )
    })? {
        let iface_name_in_msg = nl_msg.attributes.iter().find_map(|attr| {
            if let rtnetlink::packet_route::link::LinkAttribute::IfName(name) =
                attr
            {
                Some(name.as_str())
            } else {
                None
            }
        });
        if iface_name_in_msg == Some(iface_name) {
            return Ok(nl_msg.header.index);
        }
    }
    Err(NipartError::new(
        ErrorKind::Bug,
        format!("Interface {iface_name} not found"),
    ))
}
