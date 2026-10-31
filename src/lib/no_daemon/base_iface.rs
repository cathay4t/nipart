// SPDX-License-Identifier: Apache-2.0

use super::iface::nipart_iface_state_to_nispor;
use crate::{BaseInterface, InterfaceType, NipartError};

/// Apply changes to [BaseInterface] except the IP layer stuff.
pub(crate) fn apply_base_iface_link_changes(
    np_iface: &mut nispor::IfaceConf,
    apply_iface: &BaseInterface,
    cur_iface: Option<&BaseInterface>,
) -> Result<(), NipartError> {
    // We do not check current property state, nispor will ignore unchanged
    // property.

    np_iface.state = nipart_iface_state_to_nispor(apply_iface.state);
    // It is OK to use `as` action here:
    // 1. Pre-apply sanitize checker already confirmed it never exceed max_mtu,
    //    in linux kernel, the ethernet max MTU is u32::MAX.
    // 2. Verification process will complains if overflow a u32 for special
    //    interface which support MTU bigger than u32::MAX.
    np_iface.mtu = apply_iface.mtu.map(|mtu| mtu as u32);

    if apply_iface.iface_type != InterfaceType::OvsInterface {
        np_iface.controller = apply_iface.controller.clone();
    }

    // Alternative names: nispor applies the add/remove after the interface
    // exists (it fetches the fresh ifindex), so this also works for
    // interfaces created in the same apply.  Only the entries explicitly
    // listed in the desired `alt-names` are touched; other existing
    // alt-names are left alone.
    if let Some(des_alt_names) = apply_iface.alt_names.as_ref() {
        let cur_alt_names: std::collections::HashSet<&str> = cur_iface
            .and_then(|i| i.alt_names.as_ref())
            .map(|entries| {
                entries.iter().map(|entry| entry.name.as_str()).collect()
            })
            .unwrap_or_default();
        let mut np_alt_names: Vec<nispor::AltNameConf> = Vec::new();
        for des_alt_name in des_alt_names {
            let des_absent = des_alt_name.is_absent();
            let cur_exists = cur_alt_names.contains(des_alt_name.name.as_str());
            match (des_absent, cur_exists) {
                // Add a new alt-name, or remove an existing one.
                (false, false) | (true, true) => {
                    np_alt_names.push(nispor::AltNameConf::new(
                        des_alt_name.name.to_string(),
                        des_absent,
                    ));
                }
                // Already exists, or already removed: no-op.
                (false, true) | (true, false) => {}
            }
        }
        if !np_alt_names.is_empty() {
            np_iface.alt_names = np_alt_names;
        }
    }
    Ok(())
}
