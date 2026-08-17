# SPDX-License-Identifier: Apache-2.0

import nipart

from .conftest import CLI_PATH
from .testlib.cmdlib import exec_cmd
from .testlib.statelib import load_yaml, show_saved_only

SAVED_ONLY_IFACE = "saved-dummy0"
ACTIVE_IFACE = "active-dummy0"
SAVED_ROUTE_DST = "198.51.100.0/24"
SAVED_ROUTE_DST2 = "198.51.100.128/25"


def _iface_exists(iface_name):
    rc, _, _ = exec_cmd(["ip", "link", "show", iface_name], check=False)
    return rc == 0


def _route_exists(destination):
    rc, out, _ = exec_cmd(["ip", "route", "show", destination], check=False)
    return rc == 0 and destination in out


def _saved_only_iface_yaml(iface_name):
    return f"""---
interfaces:
  - name: {iface_name}
    type: dummy
    state: saved
    auto-connect: false
    mtu: 1280
"""


def test_apply_state_saved_creates_profile_without_activating():
    nipart.apply(load_yaml(_saved_only_iface_yaml(SAVED_ONLY_IFACE)))
    try:
        assert not _iface_exists(
            SAVED_ONLY_IFACE
        ), f"{SAVED_ONLY_IFACE} must not be activated by `state: saved`"
        saved_iface = show_saved_only(SAVED_ONLY_IFACE)
        assert (
            saved_iface is not None
        ), f"{SAVED_ONLY_IFACE} should be persisted in saved config"
        assert saved_iface["state"] == "saved", saved_iface

        rc, out, err = exec_cmd([CLI_PATH, "s", SAVED_ONLY_IFACE], check=False)
        assert rc == 0, f"npt s failed:\n{out}\n{err}"
        assert "state: saved" in out, out

        rc, out, err = exec_cmd(
            [CLI_PATH, "s", "--saved", SAVED_ONLY_IFACE], check=False
        )
        assert rc == 0, f"npt s --saved failed:\n{out}\n{err}"
        assert "state: saved" in out, out
        assert "mtu: 1280" in out, out
    finally:
        nipart.apply(load_yaml(f"""---
interfaces:
  - name: {SAVED_ONLY_IFACE}
    type: dummy
    state: absent
"""))


def test_apply_state_saved_persists_routes_without_applying():
    nipart.apply(load_yaml(f"""---
interfaces:
  - name: {SAVED_ONLY_IFACE}
    type: dummy
    state: saved
    auto-connect: false
routes:
  config:
    - destination: {SAVED_ROUTE_DST}
      next-hop-interface: {SAVED_ONLY_IFACE}
"""))
    try:
        assert not _route_exists(
            SAVED_ROUTE_DST
        ), f"Route via {SAVED_ONLY_IFACE} must not be applied"
        saved_state = nipart.NipartClient().query_network_state(
            nipart.NipartQueryOption.saved()
        )
        saved_routes = saved_state.get("routes", {}).get("config", [])
        assert any(
            r.get("next-hop-interface") == SAVED_ONLY_IFACE
            for r in saved_routes
        ), saved_routes

        rc, out, err = exec_cmd([CLI_PATH, "s", SAVED_ONLY_IFACE], check=False)
        assert rc == 0, f"npt s failed:\n{out}\n{err}"
        assert "state: saved" in out, out
        assert SAVED_ROUTE_DST in out, out
        assert "next-hop-interface:" in out, out
    finally:
        nipart.apply(load_yaml(f"""---
interfaces:
  - name: {SAVED_ONLY_IFACE}
    type: dummy
    state: absent
"""))


def test_apply_saved_route_via_active_iface_not_applied():
    nipart.apply(load_yaml(f"""---
interfaces:
  - name: {ACTIVE_IFACE}
    type: dummy
    state: up
routes:
  config:
    - destination: {SAVED_ROUTE_DST2}
      next-hop-interface: {ACTIVE_IFACE}
      state: saved
"""))
    try:
        assert _iface_exists(ACTIVE_IFACE)
        assert not _route_exists(
            SAVED_ROUTE_DST2
        ), "`state: saved` route must not be applied to kernel"

        rc, out, err = exec_cmd([CLI_PATH, "s", ACTIVE_IFACE], check=False)
        assert rc == 0, f"npt s failed:\n{out}\n{err}"
        assert SAVED_ROUTE_DST2 in out, out
        assert "state: saved" in out, out

        rc, out, err = exec_cmd(
            [CLI_PATH, "s", "--saved", ACTIVE_IFACE], check=False
        )
        assert rc == 0, f"npt s --saved failed:\n{out}\n{err}"
        assert "state: up" in out, out
        assert SAVED_ROUTE_DST2 in out, out
    finally:
        nipart.apply(load_yaml(f"""---
interfaces:
  - name: {ACTIVE_IFACE}
    type: dummy
    state: absent
"""))


def test_npt_up_activates_saved_only_profile():
    nipart.apply(load_yaml(_saved_only_iface_yaml(SAVED_ONLY_IFACE)))
    try:
        assert not _iface_exists(SAVED_ONLY_IFACE)
        rc, out, err = exec_cmd(
            [CLI_PATH, "up", SAVED_ONLY_IFACE], check=False
        )
        assert rc == 0, f"npt up failed:\n{out}\n{err}"
        assert _iface_exists(
            SAVED_ONLY_IFACE
        ), f"{SAVED_ONLY_IFACE} should be created by `npt up`"
    finally:
        nipart.apply(load_yaml(f"""---
interfaces:
  - name: {SAVED_ONLY_IFACE}
    type: dummy
    state: absent
"""))
