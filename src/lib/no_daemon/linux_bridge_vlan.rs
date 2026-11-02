// SPDX-License-Identifier: Apache-2.0

use crate::{BridgeVlanConfig, BridgeVlanMode, BridgeVlanTrunkTag};

fn _gen_bridge_vlan_conf(
    vlan_conf: &BridgeVlanConfig,
) -> Vec<nispor::BridgeVlanEntry> {
    let mut ret = Vec::new();
    match vlan_conf.mode {
        Some(BridgeVlanMode::Trunk) => {
            if let Some(trunk_tags) = &vlan_conf.trunk_tags {
                for trunk_tag in trunk_tags.as_slice() {
                    ret.push(trunk_tag_to_np_vlan_range(trunk_tag));
                }
            }
            if let Some(t) = vlan_conf.tag {
                ret.push(access_tag_to_np_vlan_range(t))
            }
        }
        Some(BridgeVlanMode::Access) => {
            if let Some(t) = vlan_conf.tag {
                ret.push(access_tag_to_np_vlan_range(t))
            };
        }
        _ => (),
    }

    ret
}

pub(crate) fn gen_bridge_vlan_conf(
    des_vlan_conf: &BridgeVlanConfig,
    cur_vlan_conf: Option<&BridgeVlanConfig>,
) -> Vec<nispor::BridgeVlanEntry> {
    let cur_entries = if let Some(cur_vlan_conf) = cur_vlan_conf.as_ref() {
        _gen_bridge_vlan_conf(cur_vlan_conf)
    } else {
        Vec::new()
    };
    let des_entries = _gen_bridge_vlan_conf(des_vlan_conf);

    let mut ret = Vec::new();

    // remove old entries
    for mut cur_entry in cur_entries {
        if !des_entries.contains(&cur_entry) {
            cur_entry.remove = true;
            ret.push(cur_entry)
        }
    }
    ret.extend(des_entries);
    ret
}

fn trunk_tag_to_np_vlan_range(
    trunk_tag: &BridgeVlanTrunkTag,
) -> nispor::BridgeVlanEntry {
    let mut ret = nispor::BridgeVlanEntry::default();
    match trunk_tag {
        BridgeVlanTrunkTag::Id(vid) => {
            ret.vid = Some(*vid);
        }
        BridgeVlanTrunkTag::IdRange(range) => {
            ret.vid_range = Some((range.min, range.max));
        }
    }
    ret.is_pvid = false;
    ret.is_egress_untagged = false;
    ret
}

fn access_tag_to_np_vlan_range(tag: u16) -> nispor::BridgeVlanEntry {
    let mut ret = nispor::BridgeVlanEntry::default();
    ret.vid = Some(tag);
    ret.is_pvid = true;
    ret.is_egress_untagged = true;
    ret
}
