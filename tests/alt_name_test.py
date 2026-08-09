# SPDX-License-Identifier: Apache-2.0

import nipart
import pytest

from .conftest import start_daemon, stop_daemon
from .testlib.cmdlib import exec_cmd
from .testlib.retry import retry_till_true_or_timeout
from .testlib.statelib import load_yaml, show_only

TEST_VETH = "veth-altname0"
TEST_VETH_PEER = "veth-altname1"
DEFAULT_TIMEOUT = 30


def _gen_veth_state(alt_names, iface_name=TEST_VETH, peer=TEST_VETH_PEER):
    entries = ""
    for name, state in alt_names:
        entries += f"            - name: {name}\n"
        if state:
            entries += f"              state: {state}\n"
    return load_yaml(f"""---
        interfaces:
        - name: {iface_name}
          type: veth
          state: up
          alt-names:
{entries}          veth:
            peer: {peer}
    """)


def _create_veth_pair():
    exec_cmd(
        f"ip link add {TEST_VETH} type veth peer name {TEST_VETH_PEER}".split()
    )


def _remove_veth_pair():
    exec_cmd(f"ip link del {TEST_VETH}".split(), check=False)


def _has_alt_names(iface_name, expected):
    iface = show_only(iface_name)
    if iface is None:
        return False
    alt_names = iface.get("alt-names")
    names = {entry["name"] for entry in alt_names} if alt_names else set()
    return names == set(expected)


def _kernel_alt_names(iface_name):
    rc, out, _ = exec_cmd(
        ["ip", "-d", "link", "show", iface_name], check=False
    )
    if rc != 0:
        return set()
    return {
        line.split()[-1]
        for line in out.splitlines()
        if line.strip().startswith("altname")
    }


def test_alt_name_apply_add():
    _create_veth_pair()
    try:
        # The desired order (zebra, alpha) is not kept: alt-names are
        # sorted during sanitization.
        nipart.apply(_gen_veth_state([("zebra", None), ("alpha", None)]))
        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT, _has_alt_names, TEST_VETH, ["alpha", "zebra"]
        ), "Alt-names not applied"
        assert _kernel_alt_names(TEST_VETH) == {"alpha", "zebra"}
    finally:
        _remove_veth_pair()


def test_alt_name_add():
    _create_veth_pair()
    try:
        nipart.apply(_gen_veth_state([("alpha", None)]))
        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT, _has_alt_names, TEST_VETH, ["alpha"]
        )
        # Adding a new alt-name keeps the existing one.
        nipart.apply(_gen_veth_state([("alpha", None), ("beta", None)]))
        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT,
            _has_alt_names,
            TEST_VETH,
            ["alpha", "beta"],
        ), "Added alt-name not applied / existing alt-name lost"
    finally:
        _remove_veth_pair()


def test_alt_name_change():
    _create_veth_pair()
    try:
        nipart.apply(_gen_veth_state([("alpha", None), ("beta", None)]))
        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT,
            _has_alt_names,
            TEST_VETH,
            ["alpha", "beta"],
        )
        # Remove `beta`, add `gamma`.
        nipart.apply(
            _gen_veth_state(
                [("alpha", None), ("beta", "absent"), ("gamma", None)]
            )
        )
        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT,
            _has_alt_names,
            TEST_VETH,
            ["alpha", "gamma"],
        ), "Changed alt-names (remove beta, add gamma) not applied"
    finally:
        _remove_veth_pair()


def test_alt_name_remove():
    _create_veth_pair()
    try:
        nipart.apply(_gen_veth_state([("alpha", None), ("beta", None)]))
        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT,
            _has_alt_names,
            TEST_VETH,
            ["alpha", "beta"],
        )
        nipart.apply(_gen_veth_state([("beta", "absent")]))
        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT, _has_alt_names, TEST_VETH, ["alpha"]
        ), "Removed alt-name still present"
    finally:
        _remove_veth_pair()


def test_alt_name_conflict_rejected():
    _create_veth_pair()
    try:
        # Two interfaces sharing the same alt-name must be rejected.
        state = load_yaml(f"""---
            interfaces:
            - name: {TEST_VETH}
              type: veth
              state: up
              alt-names:
                - name: shared
              veth:
                peer: {TEST_VETH_PEER}
            - name: {TEST_VETH_PEER}
              type: veth
              state: up
              alt-names:
                - name: shared
            """)
        with pytest.raises(nipart.NipartError) as err:
            nipart.apply(state)
        assert "already used by interface" in str(err.value)
    finally:
        _remove_veth_pair()


def test_boot_load_saved_alt_names():
    _create_veth_pair()
    try:
        nipart.apply(_gen_veth_state([("alpha", None), ("zebra", None)]))
        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT, _has_alt_names, TEST_VETH, ["alpha", "zebra"]
        )
        # Wipe the kernel alt-names, then restart the daemon: the saved
        # config must re-apply the alt-names at boot.
        exec_cmd(
            f"ip link property del dev {TEST_VETH} altname alpha".split(),
            check=False,
        )
        exec_cmd(
            f"ip link property del dev {TEST_VETH} altname zebra".split(),
            check=False,
        )
        assert _kernel_alt_names(TEST_VETH) == set()
        stop_daemon()
        start_daemon()
        assert retry_till_true_or_timeout(
            DEFAULT_TIMEOUT, _has_alt_names, TEST_VETH, ["alpha", "zebra"]
        ), "Saved alt-names not re-applied at boot"
    finally:
        _remove_veth_pair()
