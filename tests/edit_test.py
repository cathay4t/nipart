# SPDX-License-Identifier: Apache-2.0

import os

import nipart

from .conftest import CLI_PATH
from .testlib.cmdlib import exec_cmd
from .testlib.statelib import load_yaml

EDIT_DUMMY = "dummy-edit0"


def _apply_dummy():
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {EDIT_DUMMY}
                type: dummy
                state: up
                ipv4:
                  enabled: true
                  dhcp: false
                  address:
                    - ip: 198.51.100.10
                      prefix-length: 24
            routes:
              config:
                - destination: 198.51.100.0/24
                  next-hop-interface: {EDIT_DUMMY}
            """))


def _remove_dummy():
    nipart.apply(load_yaml(f"""---
            interfaces:
              - name: {EDIT_DUMMY}
                type: dummy
                state: absent
            """))


def _run_edit(*args):
    old_editor = os.environ.get("EDITOR")
    old_visual = os.environ.get("VISUAL")
    os.environ["EDITOR"] = "true"
    os.environ.pop("VISUAL", None)
    try:
        return exec_cmd([CLI_PATH, "edit", *args], check=False)
    finally:
        if old_editor is None:
            os.environ.pop("EDITOR", None)
        else:
            os.environ["EDITOR"] = old_editor
        if old_visual is None:
            os.environ.pop("VISUAL", None)
        else:
            os.environ["VISUAL"] = old_visual


def test_edit_saved_profile_without_change():
    _apply_dummy()
    try:
        rc, out, err = _run_edit(EDIT_DUMMY)
        assert rc == 0, f"npt edit failed:\n{out}\n{err}"
        assert "Nothing changed" in out, out
    finally:
        _remove_dummy()


def test_edit_missing_saved_profile_fails():
    rc, out, err = _run_edit("no-such-edit-profile")
    assert rc != 0, "npt edit should fail for a missing saved profile"
    assert "No interface or profile" in err, err


def test_edit_take_current():
    _apply_dummy()
    try:
        rc, out, err = _run_edit("--take-current", EDIT_DUMMY)
        assert rc == 0, f"npt edit --take-current failed:\n{out}\n{err}"
    finally:
        _remove_dummy()


def test_edit_no_daemon():
    _apply_dummy()
    try:
        rc, out, err = _run_edit("--no-daemon", EDIT_DUMMY)
        assert rc == 0, f"npt edit --no-daemon failed:\n{out}\n{err}"
    finally:
        _remove_dummy()
