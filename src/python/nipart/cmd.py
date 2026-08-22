# SPDX-License-Identifier: Apache-2.0

import json

from .schema.state_option import NipartApplyOption
from .schema.state_option import NipartQueryOption


class NipartCmdPing:
    IPC_KIND = "ping"

    def to_json(self):
        return json.dumps(
            {
                "kind": NipartCmdPing.IPC_KIND,
                "data": NipartCmdPing.IPC_KIND,
            }
        )


class NipartCmdUpInterface:
    IPC_KIND = "up-interface"

    def __init__(self, name: str):
        self.name = name

    def to_json(self):
        return json.dumps(
            {
                "kind": NipartCmdUpInterface.IPC_KIND,
                "data": {NipartCmdUpInterface.IPC_KIND: self.name},
            }
        )


class NipartCmdDownInterface:
    IPC_KIND = "down-interface"

    def __init__(self, name: str):
        self.name = name

    def to_json(self):
        return json.dumps(
            {
                "kind": NipartCmdDownInterface.IPC_KIND,
                "data": {NipartCmdDownInterface.IPC_KIND: self.name},
            }
        )


class NipartCmdWifiControl:
    IPC_KIND = "wifi-control"

    def __init__(self, control: str):
        self.control = control

    def to_json(self):
        return json.dumps(
            {
                "kind": NipartCmdWifiControl.IPC_KIND,
                "data": {NipartCmdWifiControl.IPC_KIND: self.control},
            }
        )


class NipartCmdQueryNetworkState:
    IPC_KIND = "query-network-state"

    def __init__(self, opt: NipartQueryOption):
        self.opt = opt

    def to_json(self):
        return json.dumps(
            {
                "kind": NipartCmdQueryNetworkState.IPC_KIND,
                "data": {
                    NipartCmdQueryNetworkState.IPC_KIND: self.opt.to_dict()
                },
            }
        )


class NipartCmdApplyNetworkState:
    IPC_KIND = "apply-network-state"

    def __init__(self, desired_state, opt: NipartApplyOption):
        self.desired_state = desired_state
        self.opt = opt

    def to_json(self):
        return json.dumps(
            {
                "kind": NipartCmdApplyNetworkState.IPC_KIND,
                "data": {
                    NipartCmdApplyNetworkState.IPC_KIND: (
                        self.desired_state,
                        self.opt.to_dict(),
                    )
                },
            }
        )
