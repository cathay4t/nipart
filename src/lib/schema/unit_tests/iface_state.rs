// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn test_interface_state_saved_serde() {
    assert_eq!(
        rmsd_yaml::to_string(&InterfaceState::Saved).unwrap(),
        "saved\n"
    );
    assert_eq!(
        rmsd_yaml::from_str::<InterfaceState>("saved").unwrap(),
        InterfaceState::Saved
    );
    assert!(InterfaceState::Saved.is_saved());
}
