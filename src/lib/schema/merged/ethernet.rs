// SPDX-License-Identifier: Apache-2.0

use crate::{
    BaseInterface, EthernetInterface, Interface, InterfaceState, InterfaceType,
    MergedInterface, MergedInterfaces, NipartInterface,
};

impl MergedInterfaces {
    // * When veth been marked as delete/down, its peer should also
    // * Automatically bring veth peer up when creating
    pub(crate) fn post_merge_sanitize_veth(&mut self) {
        self.bring_new_veth_peer_up();
        self.mark_veth_peer_absent_also();
    }

    fn bring_new_veth_peer_up(&mut self) {
        let mut new_veth_peers: Vec<MergedInterface> = Vec::new();
        for merged_iface in
            self.kernel_ifaces.values().filter(|i| i.current.is_none())
        {
            if let Some(Interface::Ethernet(eth_iface)) =
                merged_iface.for_apply.as_ref()
                && let Some(peer) =
                    eth_iface.veth.as_ref().map(|v| v.peer.as_str())
                && !self.kernel_ifaces.contains_key(peer)
            {
                let base_iface = BaseInterface::new(
                    peer.to_string(),
                    InterfaceType::Ethernet,
                );

                let des_iface = Interface::Ethernet(Box::new(
                    EthernetInterface::new_veth(base_iface, eth_iface.name()),
                ));

                let mut new_merged_iface =
                    match MergedInterface::new(Some(des_iface), None, None) {
                        Ok(i) => i,
                        Err(e) => {
                            log::error!(
                                "BUG: Cannot create MergedInterface for newly \
                                 created veth peer {}: {e}",
                                peer
                            );
                            continue;
                        }
                    };
                // Veth peer should be activated after creation which
                // is holding up_priority 0
                new_merged_iface.up_priority = Some(1);
                new_merged_iface.for_verify = None;

                new_veth_peers.push(new_merged_iface);
            }
        }
        for merged_iface in new_veth_peers {
            let iface_name =
                merged_iface.merged.kernel_iface_name().to_string();
            self.kernel_ifaces.insert(iface_name, merged_iface);
        }
    }

    /// When veth is marked as absent, its peer should be marked as absent
    /// if not desired.
    fn mark_veth_peer_absent_also(&mut self) {
        let mut pending_changes: Vec<String> = Vec::new();
        for iface in self.kernel_ifaces.values().filter(|i| {
            i.for_apply.as_ref().map(|i| i.is_absent()) == Some(true)
        }) {
            if let Some(Interface::Ethernet(iface)) = iface.current.as_ref()
                && let Some(peer) = iface.veth.as_ref().map(|v| v.peer.as_str())
                && let Some(peer_iface) = self.kernel_ifaces.get(peer)
                && peer_iface.desired.is_none()
            {
                pending_changes.push(peer.to_string());
            }
        }
        for iface_name in pending_changes {
            if let Some(iface) = self
                .kernel_ifaces
                .get_mut(&iface_name)
                .and_then(|i| i.for_apply.as_mut())
            {
                iface.base_iface_mut().state = InterfaceState::Absent;
            }
        }
    }
}
