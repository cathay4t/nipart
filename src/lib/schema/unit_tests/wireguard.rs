// SPDX-License-Identifier: Apache-2.0

use crate::{Interface, NetworkState, NipartInterface, WireguardInterface};

const TEST_PRIVATE_KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

fn gen_wireguard_state_with_private_key() -> NetworkState {
    let plain_yaml = format!(
        r#"---
        interfaces:
          - name: wg0
            type: wireguard
            state: up
            wireguard:
              private-key: '{TEST_PRIVATE_KEY}'"#
    );
    NetworkState::new_from_yaml(&plain_yaml).unwrap()
}

fn gen_wireguard_state_with_hidden_private_key() -> NetworkState {
    let hidden_yaml = r#"---
        interfaces:
          - name: wg0
            type: wireguard
            state: up
            wireguard:
              private-key: <_hidden_>"#;
    NetworkState::new_from_yaml(hidden_yaml).unwrap()
}

fn wireguard_iface_with_private_key() -> WireguardInterface {
    rmsd_yaml::from_str(&format!(
        r#"---
        name: wg0
        type: wireguard
        state: up
        wireguard:
          private-key: '{TEST_PRIVATE_KEY}'"#
    ))
    .unwrap()
}

fn wireguard_iface_with_hidden_private_key() -> WireguardInterface {
    rmsd_yaml::from_str(
        r#"---
        name: wg0
        type: wireguard
        state: up
        wireguard:
          private-key: <_hidden_>"#,
    )
    .unwrap()
}

#[test]
fn test_wireguard_hidden_private_key_in_merge_keeps_old_secret() {
    let mut safe_state = gen_wireguard_state_with_private_key();
    let hidden_state = gen_wireguard_state_with_hidden_private_key();

    safe_state.merge(&hidden_state).unwrap();

    let wg = safe_state
        .ifaces
        .kernel_ifaces
        .get("wg0")
        .and_then(|i| {
            if let Interface::Wireguard(w) = i {
                w.wireguard.as_ref()
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(wg.private_key.as_deref(), Some(TEST_PRIVATE_KEY));
}

#[test]
fn test_wireguard_hidden_private_key_in_gen_diff_no_diff() {
    let plain_state = gen_wireguard_state_with_private_key();
    let hidden_state = gen_wireguard_state_with_hidden_private_key();

    let diff_state = hidden_state.gen_diff(&plain_state).unwrap();

    assert!(
        !diff_state.ifaces.kernel_ifaces.contains_key("wg0"),
        "Hidden private key should not generate a diff, but got: {:?}",
        diff_state.ifaces.kernel_ifaces.get("wg0")
    );
}

#[test]
fn test_wireguard_sanitize_keeps_full_saved_config_on_diff_apply() {
    let desired = wireguard_iface_with_hidden_private_key();
    let current = wireguard_iface_with_private_key();
    let mut for_save = wireguard_iface_with_private_key();
    let mut for_apply: WireguardInterface = rmsd_yaml::from_str(
        r#"---
        name: wg0
        type: wireguard
        state: up
        wireguard:
          listen-port: 51820"#,
    )
    .unwrap();
    let mut for_verify = desired.clone();
    let mut merged = for_save.clone();

    desired
        .sanitize(
            Some(&current),
            &mut for_save,
            &mut for_apply,
            &mut for_verify,
            &mut merged,
        )
        .unwrap();

    let wg = for_save.wireguard.as_ref().unwrap();
    assert_eq!(wg.private_key.as_deref(), Some(TEST_PRIVATE_KEY));
}
