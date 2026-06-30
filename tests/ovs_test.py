# SPDX-License-Identifier: Apache-2.0

import pathlib
import time

import pytest

import nipart
from nipart import NipartClient, NipartStateKind, NipartQueryOption

from .testlib.cmdlib import exec_cmd
from .testlib.statelib import load_yaml, state_match

project_dir = pathlib.Path(__file__).parent.parent.resolve()
OVS_PLUGIN_BIN = f"{project_dir}/target/debug/nipart-plugin-ovs"
OVS_DB_SOCK = "/run/openvswitch/db.sock"

TEST_OVS_BR = "br0"
RETRY_TIMEOUT = 30


def _get_ifaces_by_name(name):
    client = NipartClient()
    state = client.query_network_state(
        NipartQueryOption(kind=NipartStateKind.RUNNING)
    )
    return [i for i in state["interfaces"] if i["name"] == name]


def _wait_for_ovs_bridge(br_name, timeout):
    for _ in range(timeout):
        ifaces = _get_ifaces_by_name(br_name)
        types = {i.get("type") for i in ifaces}
        if "ovs-bridge" in types and "ovs-interface" in types:
            return True
        time.sleep(1)
    return False


def _wait_for_ovs_bridge_gone(br_name, timeout):
    for _ in range(timeout):
        ifaces = _get_ifaces_by_name(br_name)
        if not ifaces:
            return True
        time.sleep(1)
    return False


@pytest.fixture
def ovs_bridge():
    exec_cmd(["ovs-vsctl", "add-br", TEST_OVS_BR], check=False)
    yield
    exec_cmd(["ovs-vsctl", "del-br", TEST_OVS_BR], check=False)


def test_ovs_bridge_query(ovs_bridge):
    assert _wait_for_ovs_bridge(
        TEST_OVS_BR, RETRY_TIMEOUT
    ), f"Timed out waiting for {TEST_OVS_BR} to appear in nipart query"

    br_ifaces = _get_ifaces_by_name(TEST_OVS_BR)
    assert len(br_ifaces) == 2, (
        f"Expected two entries for {TEST_OVS_BR} "
        f"(ovs-bridge + ovs-interface), got {len(br_ifaces)}"
    )

    br_map = {}
    for i in br_ifaces:
        br_map[i["type"]] = i

    # Verify ovs-bridge entry
    assert (
        "ovs-bridge" in br_map
    ), f"Missing ovs-bridge entry for {TEST_OVS_BR}"
    br_entry = br_map["ovs-bridge"]
    assert state_match(
        {"name": TEST_OVS_BR, "type": "ovs-bridge", "state": "up"},
        br_entry,
    ), f"ovs-bridge entry mismatch: {br_entry}"
    # Ports list should include br0 itself as an internal port
    assert (
        br_entry.get("bridge", {}).get("ports") is not None
    ), f"ovs-bridge {TEST_OVS_BR} missing ports in bridge config"
    assert any(
        p.get("name") == TEST_OVS_BR for p in br_entry["bridge"]["ports"]
    ), f"ovs-bridge {TEST_OVS_BR} ports list missing {TEST_OVS_BR}"

    # Verify ovs-interface entry
    assert (
        "ovs-interface" in br_map
    ), f"Missing ovs-interface entry for {TEST_OVS_BR}"
    iface_entry = br_map["ovs-interface"]
    assert state_match(
        {
            "name": TEST_OVS_BR,
            "type": "ovs-interface",
            "controller": TEST_OVS_BR,
            "controller-type": "ovs-bridge",
        },
        iface_entry,
    ), f"ovs-interface entry mismatch: {iface_entry}"


def test_ovs_bridge_create_and_remove():
    exec_cmd(["ovs-vsctl", "add-br", TEST_OVS_BR], check=False)
    try:
        assert _wait_for_ovs_bridge(
            TEST_OVS_BR, RETRY_TIMEOUT
        ), f"Timed out waiting for {TEST_OVS_BR} after creation"
        br_ifaces = _get_ifaces_by_name(TEST_OVS_BR)
        assert len(br_ifaces) == 2
    finally:
        exec_cmd(["ovs-vsctl", "del-br", TEST_OVS_BR], check=False)

    assert _wait_for_ovs_bridge_gone(
        TEST_OVS_BR, RETRY_TIMEOUT
    ), f"Timed out waiting for {TEST_OVS_BR} to be removed"


@pytest.fixture
def no_nipart_ovs_plugin():
    exec_cmd(["mount", "-o", "bind", "/dev/null", OVS_PLUGIN_BIN])
    yield
    exec_cmd(["umount", OVS_PLUGIN_BIN], check=False)


@pytest.fixture
def no_ovs_db_socket():
    exec_cmd(["mount", "-o", "bind", "/dev/null", OVS_DB_SOCK])
    yield
    exec_cmd(["umount", OVS_DB_SOCK], check=False)


def test_ovs_apply_dependency_error_plugin_not_found(
    no_nipart_ovs_plugin, restart_daemon
):
    desired_state = load_yaml("""---
        interfaces:
          - name: test-br
            type: ovs-bridge
            state: up
    """)
    with pytest.raises(nipart.NipartError) as err:
        nipart.apply(desired_state)
    assert err.value.kind == "dependency-error", (
        f"Expected dependency-error, got {err.value.kind}: {err.value.msg}"
    )


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
    rc, out, _ = exec_cmd(
        ["pgrep", "-x", "nipartd"], check=False
    )
    assert rc == 0, "Cannot find daemon PID via pgrep -x nipartd"
    daemon_pid = out.strip()
    exec_cmd(["kill", "-TERM", daemon_pid], check=False)
    for _ in range(20):
        if _count_plugin_processes() == 0:
            break
        time.sleep(0.5)
    assert _count_plugin_processes() == 0, (
        "Plugin processes were not cleaned up after daemon shutdown"
    )


def test_ovs_apply_dependency_error_daemon_not_running(no_ovs_db_socket):
    desired_state = load_yaml("""---
        interfaces:
          - name: test-br
            type: ovs-bridge
            state: up
    """)
    with pytest.raises(nipart.NipartError) as err:
        nipart.apply(desired_state)
    assert err.value.kind == "dependency-error", (
        f"Expected dependency-error, got {err.value.kind}: {err.value.msg}"
    )
