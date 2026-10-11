// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use super::*;

static TEST_DIR_ID: AtomicU64 = AtomicU64::new(0);

/// A unique, self-cleaning 0700 directory for one test.
struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "nipart_hints_test_{}_{}",
            std::process::id(),
            TEST_DIR_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create test dir");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .expect("chmod test dir");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn hint_path(&self, ssid: &str) -> PathBuf {
        path_for_ssid(&self.0, ssid)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn mac(last: u8) -> [u8; 6] {
    [0x02, 0x00, 0x00, 0x00, 0x00, last]
}

#[test]
fn encodes_unsafe_ssid_bytes() {
    let dir = Path::new("/tmp/hints");
    // SSIDs are not safe path components: `/`, spaces and non-ASCII
    // bytes are percent-encoded from their UTF-8 bytes.
    assert_eq!(
        path_for_ssid(dir, "My Wifi/5G"),
        dir.join("My%20Wifi%2F5G.hints")
    );
    assert_eq!(
        path_for_ssid(dir, "空蝉"),
        dir.join("%E7%A9%BA%E8%9D%89.hints")
    );
    // The safe set passes through unchanged.
    assert_eq!(
        path_for_ssid(dir, "abc.DEF_123-xyz"),
        dir.join("abc.DEF_123-xyz.hints")
    );
}

#[test]
fn encoded_name_cannot_escape_the_hints_dir() {
    let dir = Path::new("/tmp/hints");
    let path = path_for_ssid(dir, "../../etc/passwd");
    assert_eq!(path.parent(), Some(dir));
    assert_eq!(
        path.file_name().unwrap().to_string_lossy(),
        "..%2F..%2Fetc%2Fpasswd.hints"
    );
}

#[test]
fn record_and_load_roundtrip_sorts_and_dedupes() {
    let dir = TestDir::new();
    record_bss_on_day(dir.path(), "Home", &mac(1), 5180, 100).unwrap();
    record_bss_on_day(dir.path(), "Home", &mac(2), 2412, 100).unwrap();
    record_bss_on_day(dir.path(), "Home", &mac(3), 2412, 100).unwrap();

    assert_eq!(
        load_hints_on_day(dir.path(), "Home", 100).frequencies_mhz,
        vec![2412, 5180]
    );
    // Another SSID has its own file.
    assert!(load_hints_on_day(dir.path(), "Other", 100).is_empty());
}

#[test]
fn load_hints_returns_parsed_known_bss() {
    let dir = TestDir::new();
    record_bss_on_day(dir.path(), "Home", &mac(1), 5180, 100).unwrap();
    record_bss_on_day(dir.path(), "Home", &mac(2), 2412, 100).unwrap();

    let loaded = load_hints_on_day(dir.path(), "Home", 100);
    assert_eq!(loaded.known_bss, vec![(mac(1), 5180), (mac(2), 2412)]);
    assert_eq!(loaded.frequencies_mhz, vec![2412, 5180]);
}

#[test]
fn load_hints_skips_malformed_bssid_entries() {
    let dir = TestDir::new();
    let path = dir.hint_path("Home");
    let hints = PersistedHints {
        version: 1,
        ssid: "Home".to_string(),
        updated_on: 100,
        bss: vec![
            StoredBss {
                bssid: "not-a-mac".to_string(),
                frequency_mhz: 2412,
            },
            StoredBss {
                bssid: mac_to_string(&mac(1)),
                frequency_mhz: 5180,
            },
        ],
    };
    fs::write(&path, rmsd_yaml::to_string(&hints).unwrap()).unwrap();

    let loaded = load_hints_on_day(dir.path(), "Home", 100);
    assert_eq!(loaded.known_bss, vec![(mac(1), 5180)]);
    assert_eq!(loaded.frequencies_mhz, vec![5180]);
}

#[test]
fn duplicate_bssid_entries_are_deduplicated() {
    let dir = TestDir::new();
    let path = dir.hint_path("Home");
    let hints = PersistedHints {
        version: 1,
        ssid: "Home".to_string(),
        updated_on: 100,
        bss: vec![
            StoredBss {
                bssid: mac_to_string(&mac(1)),
                frequency_mhz: 5180,
            },
            StoredBss {
                bssid: mac_to_string(&mac(1)),
                frequency_mhz: 5180,
            },
        ],
    };
    fs::write(&path, rmsd_yaml::to_string(&hints).unwrap()).unwrap();

    let loaded = load_hints_on_day(dir.path(), "Home", 100);
    assert_eq!(loaded.known_bss, vec![(mac(1), 5180)]);
}

#[test]
fn record_creates_the_hints_dir_with_restrictive_permissions() {
    let dir = TestDir::new();
    let nested = dir.path().join("wifi");
    record_bss_on_day(&nested, "Home", &mac(1), 5180, 100).unwrap();

    assert!(nested.join("Home.hints").exists());
    assert_eq!(fs::metadata(&nested).unwrap().mode() & 0o777, 0o700);
    let file_mode = fs::metadata(nested.join("Home.hints")).unwrap().mode();
    assert_eq!(file_mode & 0o777, 0o600);
}

#[test]
fn same_bss_same_day_does_not_rewrite() {
    let dir = TestDir::new();
    record_bss_on_day(dir.path(), "Home", &mac(1), 5180, 100).unwrap();
    let path = dir.hint_path("Home");
    let modified = fs::metadata(&path).unwrap().modified().unwrap();

    record_bss_on_day(dir.path(), "Home", &mac(1), 5180, 100).unwrap();

    assert_eq!(
        fs::metadata(&path).unwrap().modified().unwrap(),
        modified,
        "a known BSS on the same day must not rewrite the file"
    );
}

#[test]
fn same_bss_next_day_refreshes_only_the_timestamp() {
    let dir = TestDir::new();
    record_bss_on_day(dir.path(), "Home", &mac(1), 5180, 100).unwrap();
    record_bss_on_day(dir.path(), "Home", &mac(1), 5180, 101).unwrap();

    let hints = read_hints(&dir.hint_path("Home"), "Home").unwrap();
    assert_eq!(hints.updated_on, 101);
    assert_eq!(hints.bss.len(), 1);
    assert_eq!(hints.bss[0].bssid, mac_to_string(&mac(1)));
    assert_eq!(hints.bss[0].frequency_mhz, 5180);
}

#[test]
fn frequency_change_updates_the_existing_entry() {
    let dir = TestDir::new();
    record_bss_on_day(dir.path(), "Home", &mac(1), 5180, 100).unwrap();
    record_bss_on_day(dir.path(), "Home", &mac(1), 2412, 100).unwrap();

    let hints = read_hints(&dir.hint_path("Home"), "Home").unwrap();
    assert_eq!(hints.bss.len(), 1, "same BSSID must not be duplicated");
    assert_eq!(hints.bss[0].frequency_mhz, 2412);
    assert_eq!(
        load_hints_on_day(dir.path(), "Home", 100).frequencies_mhz,
        vec![2412]
    );
}

#[test]
fn bss_list_is_capped_dropping_the_oldest() {
    let dir = TestDir::new();
    for last in 1..=(MAX_BSS_PER_SSID as u8 + 1) {
        record_bss_on_day(dir.path(), "Home", &mac(last), 2412, 100).unwrap();
    }

    let hints = read_hints(&dir.hint_path("Home"), "Home").unwrap();
    assert_eq!(hints.bss.len(), MAX_BSS_PER_SSID);
    assert_eq!(hints.bss[0].bssid, mac_to_string(&mac(2)));
    assert_eq!(
        hints.bss[MAX_BSS_PER_SSID - 1].bssid,
        mac_to_string(&mac(MAX_BSS_PER_SSID as u8 + 1))
    );
}

#[test]
fn expired_hints_are_deleted_on_load() {
    let dir = TestDir::new();
    record_bss_on_day(dir.path(), "Home", &mac(1), 5180, 100).unwrap();

    // Exactly at the TTL the hints are still valid.
    assert_eq!(
        load_hints_on_day(dir.path(), "Home", 100 + HINT_TTL_DAYS)
            .frequencies_mhz,
        vec![5180]
    );
    assert!(dir.hint_path("Home").exists());

    // One day past the TTL they are ignored and removed.
    assert!(
        load_hints_on_day(dir.path(), "Home", 100 + HINT_TTL_DAYS + 1)
            .is_empty()
    );
    assert!(!dir.hint_path("Home").exists());
}

#[test]
fn expired_hints_are_replaced_by_a_fresh_record() {
    let dir = TestDir::new();
    record_bss_on_day(dir.path(), "Home", &mac(1), 5180, 100).unwrap();
    record_bss_on_day(
        dir.path(),
        "Home",
        &mac(2),
        2412,
        100 + HINT_TTL_DAYS + 1,
    )
    .unwrap();

    let hints = read_hints(&dir.hint_path("Home"), "Home").unwrap();
    assert_eq!(hints.updated_on, 100 + HINT_TTL_DAYS + 1);
    // The expired entry is not merged into the fresh file.
    assert_eq!(hints.bss.len(), 1);
    assert_eq!(hints.bss[0].bssid, mac_to_string(&mac(2)));
}

#[test]
fn is_expired_is_day_granular() {
    assert!(!is_expired(100, 100));
    assert!(!is_expired(100, 100 + HINT_TTL_DAYS));
    assert!(is_expired(100, 100 + HINT_TTL_DAYS + 1));
    // A clock going backwards never expires a hint.
    assert!(!is_expired(100, 99));
}

#[test]
fn corrupt_files_are_ignored() {
    let dir = TestDir::new();
    let path = dir.hint_path("Home");

    fs::write(&path, "not: [valid yaml").unwrap();
    assert!(load_hints_on_day(dir.path(), "Home", 100).is_empty());

    fs::write(&path, "").unwrap();
    assert!(load_hints_on_day(dir.path(), "Home", 100).is_empty());
}

#[test]
fn wrong_version_is_ignored() {
    let dir = TestDir::new();
    write_raw_hints(&dir.hint_path("Home"), 99, "Home", 100, &[]);

    assert!(load_hints_on_day(dir.path(), "Home", 100).is_empty());
}

#[test]
fn ssid_mismatch_is_ignored() {
    let dir = TestDir::new();
    write_raw_hints(&dir.hint_path("Home"), 1, "Other", 100, &[mac(1)]);

    assert!(load_hints_on_day(dir.path(), "Home", 100).is_empty());
}

#[test]
fn oversized_files_are_ignored() {
    let dir = TestDir::new();
    let path = dir.hint_path("Home");
    fs::write(&path, vec![b'x'; (MAX_HINTS_FILE_SIZE + 1) as usize]).unwrap();

    assert!(load_hints_on_day(dir.path(), "Home", 100).is_empty());
}

#[test]
fn symlinked_files_are_ignored() {
    let dir = TestDir::new();
    let target = dir.hint_path("Home");
    record_bss_on_day(dir.path(), "Home", &mac(1), 5180, 100).unwrap();

    let symlink_path = dir.hint_path("Sneaky");
    symlink(&target, &symlink_path).unwrap();
    assert!(load_hints_on_day(dir.path(), "Sneaky", 100).is_empty());
}

#[test]
fn group_or_world_writable_files_are_ignored() {
    let dir = TestDir::new();
    let path = dir.hint_path("Home");
    record_bss_on_day(dir.path(), "Home", &mac(1), 5180, 100).unwrap();

    fs::set_permissions(&path, fs::Permissions::from_mode(0o666)).unwrap();
    assert!(load_hints_on_day(dir.path(), "Home", 100).is_empty());
}

#[test]
fn remove_is_idempotent() {
    let dir = TestDir::new();
    record_bss_on_day(dir.path(), "Home", &mac(1), 5180, 100).unwrap();
    let path = dir.hint_path("Home");
    assert!(path.exists());

    remove_in(dir.path(), "Home");
    assert!(!path.exists());
    remove_in(dir.path(), "Home");
}

#[test]
fn written_file_carries_no_secret_or_interface_fields() {
    let dir = TestDir::new();
    record_bss_on_day(dir.path(), "Home", &mac(1), 5180, 100).unwrap();
    let content = fs::read_to_string(dir.hint_path("Home")).unwrap();

    for forbidden in ["password", "interface", "psk", "pmk", "rsne"] {
        assert!(
            !content.contains(forbidden),
            "'{forbidden}' must not be stored: {content}"
        );
    }
    assert!(content.contains("updated_on"));
    assert!(content.contains("bssid"));
    assert!(content.contains("frequency_mhz"));
}

fn write_raw_hints(
    path: &Path,
    version: u32,
    ssid: &str,
    updated_on: u64,
    bss_ids: &[[u8; 6]],
) {
    let hints = PersistedHints {
        version,
        ssid: ssid.to_string(),
        updated_on,
        bss: bss_ids
            .iter()
            .map(|bssid| StoredBss {
                bssid: mac_to_string(bssid),
                frequency_mhz: 5180,
            })
            .collect(),
    };
    fs::write(path, rmsd_yaml::to_string(&hints).unwrap()).unwrap();
}
