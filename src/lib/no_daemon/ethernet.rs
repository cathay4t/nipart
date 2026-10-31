// SPDX-License-Identifier: Apache-2.0

use crate::{EthernetInterface, NipartError, NipartInterface};

pub(crate) fn apply_ethernet_conf(
    mut np_iface: nispor::IfaceConf,
    iface: &EthernetInterface,
) -> Result<Vec<nispor::IfaceConf>, NipartError> {
    if iface.is_up()
        && let Some(peer) = iface.veth.as_ref().map(|v| v.peer.as_str())
    {
        np_iface.iface_type = Some(nispor::IfaceType::Veth);
        let mut np_veth_conf = nispor::VethConf::default();
        np_veth_conf.peer = peer.to_string();
        np_iface.veth = Some(np_veth_conf);

        let mut peer_np_iface = nispor::IfaceConf::default();
        peer_np_iface.name = peer.to_string();
        peer_np_iface.iface_type = Some(nispor::IfaceType::Veth);
        peer_np_iface.state = nispor::IfaceState::Up;
        let mut np_veth_conf = nispor::VethConf::default();
        np_veth_conf.peer = iface.kernel_iface_name().to_string();
        peer_np_iface.veth = Some(np_veth_conf);
        Ok(vec![np_iface, peer_np_iface])
    } else {
        Ok(vec![np_iface])
    }
}
