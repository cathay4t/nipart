// SPDX-License-Identifier: Apache-2.0

//! Per-SSID WIFI quick-start hints.
//!
//! Every time shuli reports a successful connection, the plugin records
//! the BSS (BSSID + frequency) in `/var/lib/nipart/wifi/<ssid>.hints`.
//! On boot/plugin start the stored frequencies are attached to the shuli
//! [`NetworkConfig`](shuli::NetworkConfig) so the first scan only sweeps
//! the channels the SSID has been seen on; shuli falls back to a full
//! scan when nothing configured is found there. See
//! `docs/english/design/wifi_quick_start.md`.
//!
//! Hints are best effort: any read/validation/parse failure means "no
//! hints" and the normal full scan proceeds.

use std::{
    fmt::Write as _,
    fs::{self, DirBuilder, Metadata, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nipart::{ErrorKind, NipartError};
use serde::{Deserialize, Serialize};

/// Directory holding the per-SSID hint files.
pub(crate) const WIFI_HINTS_DIR: &str = "/var/lib/nipart/wifi";

/// A hint file not content-updated for more than this many days is
/// stale: it is ignored and deleted when its SSID is applied.
const HINT_TTL_DAYS: u64 = 30;
/// Maximum number of BSS entries remembered per SSID.
const MAX_BSS_PER_SSID: usize = 16;
/// Refuse larger files before parsing (a hint file is ~0.5 KiB).
const MAX_HINTS_FILE_SIZE: u64 = 64 * 1024;
/// On-disk format version.
const HINTS_VERSION: u32 = 1;
/// Linux `O_NOFOLLOW` (`std::fs` does not export the raw constant).
const O_NOFOLLOW: i32 = 0o400000;
const SECS_PER_DAY: u64 = 24 * 60 * 60;

/// One parsed hint file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PersistedHints {
    version: u32,
    /// Self-describing: cross-checked against the requested SSID so a
    /// renamed/forged file is discarded.
    ssid: String,
    /// UTC day number (days since 1970-01-01) of the last content
    /// change. Day granularity keeps the steady-state write rate at
    /// most one write per day.
    updated_on: u64,
    /// Every BSS of the SSID shuli has successfully connected to.
    bss: Vec<StoredBss>,
}

/// One BSS entry of a hint file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StoredBss {
    /// MAC address in the `aa:bb:cc:dd:ee:ff` form for readable YAML.
    bssid: String,
    frequency_mhz: u32,
}

/// Hints loaded from a per-SSID file, ready to attach to shuli.
///
/// Deliberately shuli-free so the file handling stays unit-testable
/// without the shuli types; `apply.rs` maps it onto
/// `NetworkConfigHints`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LoadedHints {
    /// Deduplicated frequencies (MHz) of the stored BSSes.
    pub(crate) frequencies_mhz: Vec<u32>,
    /// Stored BSSes as `(BSSID, frequency MHz)`.
    pub(crate) known_bss: Vec<([u8; 6], u32)>,
}

impl LoadedHints {
    pub(crate) fn is_empty(&self) -> bool {
        self.frequencies_mhz.is_empty() && self.known_bss.is_empty()
    }
}

/// Record a successful connection to `bssid` at `freq_mhz`.
///
/// Best effort by contract: the caller logs and continues on error.
/// The write is skipped when the BSS is already recorded and the file
/// was already updated today, so a stable connection costs at most one
/// write per day.
pub(crate) fn record_bss(
    ssid: &str,
    bssid: &[u8; 6],
    freq_mhz: u32,
) -> Result<(), NipartError> {
    record_bss_on_day(
        Path::new(WIFI_HINTS_DIR),
        ssid,
        bssid,
        freq_mhz,
        days_since_epoch_utc(),
    )
}

/// Frequencies and known BSSes to attach as shuli hints for `ssid`.
///
/// Returns empty hints when there is no usable hint file. An expired
/// file is deleted here (the SSID is being applied).
pub(crate) fn load_hints(ssid: &str) -> LoadedHints {
    load_hints_on_day(Path::new(WIFI_HINTS_DIR), ssid, days_since_epoch_utc())
}

/// Delete the hint file of `ssid` (best effort).
pub(crate) fn remove(ssid: &str) {
    remove_in(Path::new(WIFI_HINTS_DIR), ssid);
}

fn record_bss_on_day(
    dir: &Path,
    ssid: &str,
    bssid: &[u8; 6],
    freq_mhz: u32,
    today: u64,
) -> Result<(), NipartError> {
    let path = path_for_ssid(dir, ssid);
    // An expired file is replaced by a fresh one instead of merged.
    let previous = read_hints(&path, ssid)
        .filter(|hints| !is_expired(hints.updated_on, today));
    let (hints, changed) =
        merge_bss(ssid, previous, &mac_to_string(bssid), freq_mhz, today);
    if !changed {
        return Ok(());
    }
    create_hints_dir(dir)?;
    write_hints(&path, &hints)
}

fn load_hints_on_day(dir: &Path, ssid: &str, today: u64) -> LoadedHints {
    let path = path_for_ssid(dir, ssid);
    let Some(hints) = read_hints(&path, ssid) else {
        return LoadedHints::default();
    };
    if is_expired(hints.updated_on, today) {
        log::debug!(
            "WIFI quick-start hints {} expired; removing them",
            path.display()
        );
        if let Err(e) = fs::remove_file(&path) {
            log::debug!("Failed to remove {}: {e}", path.display());
        }
        return LoadedHints::default();
    }
    let mut known_bss: Vec<([u8; 6], u32)> = Vec::new();
    for bss in &hints.bss {
        let Some(bssid) = mac_from_string(&bss.bssid) else {
            log::debug!(
                "Ignoring WIFI hint entry with malformed BSSID '{}' in {}",
                bss.bssid,
                path.display()
            );
            continue;
        };
        if !known_bss.iter().any(|(existing, _)| *existing == bssid) {
            known_bss.push((bssid, bss.frequency_mhz));
        }
    }
    let mut frequencies_mhz: Vec<u32> = known_bss
        .iter()
        .map(|(_, freq_mhz)| *freq_mhz)
        .filter(|freq| *freq != 0)
        .collect();
    frequencies_mhz.sort_unstable();
    frequencies_mhz.dedup();
    LoadedHints {
        frequencies_mhz,
        known_bss,
    }
}

fn remove_in(dir: &Path, ssid: &str) {
    let path = path_for_ssid(dir, ssid);
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::debug!("Failed to remove {}: {e}", path.display()),
    }
}

/// Merge one connected BSS into the hint set.
///
/// Returns the updated hint set and whether it must be written out: a
/// known BSS with the same frequency only triggers a write when the day
/// changed (the `updated_on` refresh).
fn merge_bss(
    ssid: &str,
    previous: Option<PersistedHints>,
    bssid: &str,
    frequency_mhz: u32,
    today: u64,
) -> (PersistedHints, bool) {
    let (mut bss, previous_day) = match previous {
        Some(hints) => (hints.bss, Some(hints.updated_on)),
        None => (Vec::new(), None),
    };
    let mut content_changed = false;
    match bss.iter_mut().find(|entry| entry.bssid == bssid) {
        Some(entry) => {
            if entry.frequency_mhz != frequency_mhz {
                entry.frequency_mhz = frequency_mhz;
                content_changed = true;
            }
        }
        None => {
            bss.push(StoredBss {
                bssid: bssid.to_string(),
                frequency_mhz,
            });
            content_changed = true;
        }
    }
    if bss.len() > MAX_BSS_PER_SSID {
        bss.drain(..bss.len() - MAX_BSS_PER_SSID);
        content_changed = true;
    }
    (
        PersistedHints {
            version: HINTS_VERSION,
            ssid: ssid.to_string(),
            updated_on: today,
            bss,
        },
        content_changed || previous_day != Some(today),
    )
}

/// Parse and validate a hint file.
///
/// Any failure logs at debug level and returns `None` (the caller then
/// performs a normal full scan).
fn read_hints(path: &Path, ssid: &str) -> Option<PersistedHints> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::debug!("Failed to stat {}: {e}", path.display());
            }
            return None;
        }
    };
    if !metadata.is_file() {
        log::debug!("Ignoring non-regular WIFI hints {}", path.display());
        return None;
    }
    if metadata.len() > MAX_HINTS_FILE_SIZE {
        log::debug!(
            "Ignoring oversized WIFI hints {} ({} bytes)",
            path.display(),
            metadata.len()
        );
        return None;
    }
    if metadata.mode() & 0o022 != 0 {
        log::debug!(
            "Ignoring group/world writable WIFI hints {}",
            path.display()
        );
        return None;
    }
    if !owned_by_directory(path, &metadata) {
        log::debug!(
            "Ignoring WIFI hints {} not owned by its directory owner",
            path.display()
        );
        return None;
    }
    let mut content = String::new();
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW)
        .open(path)
        .map_err(|e| {
            log::debug!("Failed to open {}: {e}", path.display());
            e
        })
        .ok()?;
    file.read_to_string(&mut content)
        .map_err(|e| log::debug!("Failed to read {}: {e}", path.display()))
        .ok()?;
    let hints = rmsd_yaml::from_str::<PersistedHints>(&content)
        .map_err(|e| {
            log::debug!("Ignoring malformed WIFI hints {}: {e}", path.display())
        })
        .ok()?;
    if hints.version != HINTS_VERSION {
        log::debug!(
            "Ignoring WIFI hints {} with unsupported version {}",
            path.display(),
            hints.version
        );
        return None;
    }
    if hints.ssid != ssid {
        log::debug!(
            "Ignoring WIFI hints {}: embedded SSID '{}' does not match \
             requested '{}'",
            path.display(),
            hints.ssid,
            ssid
        );
        return None;
    }
    Some(hints)
}

/// Whether a file's owner matches its directory's owner.
///
/// In production the directory is created by the plugin as root (0700),
/// so this is the "owned by root" rule without hardcoding root, which
/// would break unit tests running as a normal user.
fn owned_by_directory(path: &Path, metadata: &Metadata) -> bool {
    path.parent()
        .and_then(|dir| fs::metadata(dir).ok())
        .is_some_and(|dir_metadata| dir_metadata.uid() == metadata.uid())
}

fn write_hints(path: &Path, hints: &PersistedHints) -> Result<(), NipartError> {
    let yaml = rmsd_yaml::to_string(hints).map_err(|e| {
        NipartError::new(
            ErrorKind::Bug,
            format!("Failed to serialize WIFI quick-start hints: {e}"),
        )
    })?;
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp_path =
        path.with_file_name(format!(".{file_name}.tmp.{}", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp_path)?;
        file.write_all(yaml.as_bytes())?;
        fs::rename(&tmp_path, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp_path);
        return Err(NipartError::new(
            ErrorKind::PluginFailure,
            format!(
                "Failed to save WIFI quick-start hints {}: {e}",
                path.display()
            ),
        ));
    }
    log::debug!("Saved WIFI quick-start hints to {}", path.display());
    Ok(())
}

fn create_hints_dir(dir: &Path) -> Result<(), NipartError> {
    if dir.exists() {
        return Ok(());
    }
    let mut builder = DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder.create(dir).map_err(|e| {
        NipartError::new(
            ErrorKind::PluginFailure,
            format!(
                "Failed to create WIFI quick-start hints dir {}: {e}",
                dir.display()
            ),
        )
    })
}

/// `<dir>/<percent-encoded-ssid>.hints`.
///
/// An SSID is up to 32 UTF-8 bytes and is not a safe path component
/// (`/`, spaces, leading dots). Every byte outside `[A-Za-z0-9._-]`
/// becomes `%XX` from its UTF-8 bytes.
fn path_for_ssid(dir: &Path, ssid: &str) -> PathBuf {
    dir.join(format!("{}.hints", encode_ssid(ssid)))
}

fn encode_ssid(ssid: &str) -> String {
    let mut encoded = String::with_capacity(ssid.len());
    for byte in ssid.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' => {
                encoded.push(byte as char);
            }
            _ => {
                write!(encoded, "%{byte:02X}").ok();
            }
        }
    }
    encoded
}

fn mac_to_string(mac: &[u8; 6]) -> String {
    mac.iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Parse the `aa:bb:cc:dd:ee:ff` form written by [`mac_to_string`].
fn mac_from_string(mac: &str) -> Option<[u8; 6]> {
    let mut bytes = [0u8; 6];
    let mut parts = mac.split(':');
    for byte in &mut bytes {
        let part = parts.next()?;
        if part.len() != 2 {
            return None;
        }
        *byte = u8::from_str_radix(part, 16).ok()?;
    }
    if parts.next().is_some() {
        return None;
    }
    Some(bytes)
}

fn days_since_epoch_utc() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() / SECS_PER_DAY)
        .unwrap_or(0)
}

fn is_expired(updated_on: u64, today: u64) -> bool {
    today.saturating_sub(updated_on) > HINT_TTL_DAYS
}

#[cfg(test)]
#[path = "unit_tests/hints.rs"]
mod tests;
