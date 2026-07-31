# SPDX-License-Identifier: Apache-2.0

import errno
import logging

from .client import NipartClient
from .schema.state_option import NipartApplyOption

_LOG = logging.getLogger(__name__)


def apply(desired_state, *, verify_change=True):
    try:
        cli = NipartClient()
    except OSError as e:
        if e.errno in (errno.ENOENT, errno.ECONNREFUSED):
            _LOG.warning("apply: daemon not running, skipping")
            return
        raise
    opt = NipartApplyOption(verify_change=verify_change)
    cli.apply_network_state(desired_state, opt)
