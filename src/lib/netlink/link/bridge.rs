// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nispor project
// (https://github.com/nispor/nispor) and nmstate project
// (https://nmstate.io/) which are under license of Apache 2.0.

use rtnetlink::packet_route::link::{
    AfSpecBridge, BridgeMulticastRouterType as NlBridgeMulticastRouterType,
    BridgeStpState as NlBridgeStpState, BridgeVlanInfo, BridgeVlanInfoFlags,
    InfoBridge, InfoBridgePort, InfoData, VlanProtocol as NlVlanProtocol,
};

use crate::{
    BridgeVlanConfig, BridgeVlanMode, BridgeVlanRange, BridgeVlanTrunkTag,
    LinuxBridgeConfig, LinuxBridgeMulticastRouterType, LinuxBridgeOptions,
    LinuxBridgePortConfig, LinuxBridgeStpOptions, VlanProtocol,
    netlink::mac::format_mac_address,
};

const DEFAULT_USER_HZ: u32 = 100;

