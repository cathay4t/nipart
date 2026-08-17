# SPDX-License-Identifier: Apache-2.0

from .client import NipartClient
from .schema.state_option import NipartQueryOption


def show():
    client = NipartClient()
    opt = NipartQueryOption.running()
    return client.query_network_state(opt)
