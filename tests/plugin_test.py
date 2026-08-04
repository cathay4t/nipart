# SPDX-License-Identifier: Apache-2.0

import os
import time

from .testlib.cmdlib import exec_cmd

DAEMON_PID_FILE = "/var/run/nipart/nipart.pid"


def _count_plugin_processes():
    rc, out, _ = exec_cmd(
        ["pgrep", "-f", "nipart-plugin"], check=False
    )
    if rc == 0:
        return len(out.strip().splitlines())
    return 0


def test_no_orphan_plugins_after_daemon_stop(restart_daemon):
    assert _count_plugin_processes() > 0, (
        "Expected plugin processes to be running"
    )
    assert os.path.exists(DAEMON_PID_FILE), "Cannot find daemon PID file"
    with open(DAEMON_PID_FILE) as f:
        daemon_pid = f.read().strip()
    assert daemon_pid, "Daemon PID file is empty"
    exec_cmd(["kill", "-TERM", daemon_pid], check=False)
    for _ in range(20):
        if _count_plugin_processes() == 0:
            break
        time.sleep(0.5)
    assert _count_plugin_processes() == 0, (
        "Plugin processes were not cleaned up after daemon shutdown"
    )
