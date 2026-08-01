# SPDX-License-Identifier: Apache-2.0

import os
import shutil
import pathlib
import subprocess
import sys
import time

import pytest

from .testlib.cmdlib import exec_cmd
from .testlib.retry import retry_till_true_or_timeout

project_dir = pathlib.Path(__file__).parent.parent.resolve()
sys.path.insert(0, f"{project_dir}/src/python")

from nipart import NipartClient  # noqa: E402

DAEMON_LOG = "/tmp/nipart_test_daemon.log"
CLI_PATH = f"{project_dir}/target/debug/npt"
DAEMON_PID_FILE = "/var/run/nipart/nipart.pid"
DAEMON_BIN_PATH = f"{project_dir}/target/debug/nipart"


@pytest.fixture(scope="session", autouse=True)
def test_env_setup(backup_config, run_daemon):
    yield


@pytest.fixture(scope="session")
def backup_config():
    if os.path.isdir("/etc/nipart.before_test"):
        shutil.rmtree("/etc/nipart.before_test")
    if os.path.isdir("/etc/nipart"):
        os.rename("/etc/nipart", "/etc/nipart.before_test")
    yield
    if os.path.isdir("/etc/nipart") and os.path.isdir(
        "/etc/nipart.before_test"
    ):
        shutil.rmtree("/etc/nipart")
        os.rename("/etc/nipart.before_test", "/etc/nipart")


@pytest.fixture(scope="session")
def run_daemon():
    daemon_proc = subprocess.Popen(
        DAEMON_BIN_PATH,
        stdout=sys.stdout,
        stderr=open(DAEMON_LOG, "w"),
        start_new_session=True,
    )
    time.sleep(1)
    retry_till_true_or_timeout(30, check_daemon_connection)
    yield
    daemon_proc.terminate()
    try:
        daemon_proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        daemon_proc.kill()
        daemon_proc.wait()


def check_daemon_connection():
    try:
        client = NipartClient()
        return client.ping() == "pong"
    except Exception:
        return False


def daemon_ping():
    rc, out, _ = exec_cmd([CLI_PATH, "ping"], check=False)
    return rc == 0 and "pong" in out


def _wait_daemon_stopped():
    for _ in range(20):
        if not daemon_ping():
            return
        time.sleep(0.5)


def _wait_daemon_ready():
    for _ in range(40):
        if daemon_ping():
            return
        time.sleep(1)
    raise RuntimeError("Daemon did not become ready in time")


def start_daemon():
    log_f = open(DAEMON_LOG, "a")
    subprocess.Popen(
        [DAEMON_BIN_PATH],
        stdout=subprocess.DEVNULL,
        stderr=log_f,
    )
    _wait_daemon_ready()


def stop_daemon():
    if os.path.exists(DAEMON_PID_FILE):
        with open(DAEMON_PID_FILE) as f:
            pid = f.read().strip()
        if pid:
            exec_cmd(["kill", "-TERM", pid], check=False)
            _wait_daemon_exited(pid)
            return
    _wait_daemon_stopped()


def _wait_daemon_exited(pid):
    for _ in range(40):
        try:
            os.kill(int(pid), 0)
        except ProcessLookupError:
            return
        time.sleep(0.5)
    exec_cmd(["kill", "-KILL", pid], check=False)
    _wait_daemon_stopped()


@pytest.fixture
def restart_daemon():
    stop_daemon()
    start_daemon()
    yield
    stop_daemon()
    start_daemon()


REPORT_HEADER = """OS: {osname}
Kernel: {kernel_ver}
"""


def _get_osname():
    with open("/etc/os-release") as os_release:
        for line in os_release.readlines():
            if line.startswith("PRETTY_NAME="):
                return line.split("=", maxsplit=1)[1].strip().strip('"')
    return ""


def _get_kernel_ver():
    return exec_cmd("uname -r".split())[1]


def pytest_report_header(config):
    return REPORT_HEADER.format(
        osname=_get_osname(),
        kernel_ver=_get_kernel_ver(),
    )
