<!-- vim-markdown-toc GFM -->

* [WIFI Quick Start (Hints)](#wifi-quick-start-hints)
    * [Motivation](#motivation)
    * [Goals](#goals)
    * [Non-Goals](#non-goals)
    * [Design Overview](#design-overview)
    * [Hint File](#hint-file)
        * [Location and Name](#location-and-name)
        * [Format](#format)
        * [Validation](#validation)
        * [Write Path](#write-path)
        * [Read Path](#read-path)
        * [Lifecycle and Cleanup](#lifecycle-and-cleanup)
        * [Atomicity](#atomicity)
    * [Attaching Hints to shuli](#attaching-hints-to-shuli)
        * [Why Not Scan-Free Hints](#why-not-scan-free-hints)
        * [Hint-Matched Selection](#hint-matched-selection)
        * [Post-Connect Best-BSS Scan](#post-connect-best-bss-scan)
    * [Security](#security)
    * [Failure Handling](#failure-handling)
    * [Interaction with the Monitor and Boot Activation](#interaction-with-the-monitor-and-boot-activation)
    * [Testing](#testing)
    * [Future Work](#future-work)

<!-- vim-markdown-toc -->

# WIFI Quick Start (Hints)

This document describes the WIFI quick-start design: remembering the
BSSes a saved WIFI profile has successfully connected to and using them
to restrict the first scan after boot or plugin restart. It replaces the
`/var/lib/nipart/wifi_quick_boot.yml` sketch in
[plan.md](plan.md).

Implementation: `src/plugin-wifi/hints.rs` and the hint plumbing in
`src/plugin-wifi/apply.rs` on the nipart side; the `KnownBss` hint
storage, hint-matched selection and post-connect scan in the `shuli`
crate (`config.rs`, `scan.rs`, `roam.rs`, `client/wifi_iface.rs`).

## Motivation

A cold WIFI start costs a full channel scan before shuli can pick an AP.
The scan is the slowest part of the boot path, and the information needed
to skip most of it is already known from the previous run: which BSSes of
an SSID this machine has joined, and on which channels they live.

nipart therefore records the BSSID and frequency of every successful
connection in a per-SSID file under `/var/lib/nipart/wifi/`. On the next
apply (boot hand-off, daemon restart, `npt up`) the stored frequencies
and BSSIDs are attached to the shuli network configuration as scan
hints. shuli restricts its first scan to the stored channels, connects
to the strongest visible BSS the client already knows, falls back to a
full scan when none is visible, and - once connected to a
managed-roaming BSS - runs a one-shot scan for a better BSS.

## Goals

* Shorten the first scan after boot/plugin start for profiles that were
  connected before.
* Never make connectivity worse: stale, missing, or corrupt hints must
  only cost one quick scan, never fail the boot.
* Keep credentials out of the hint files and keep the files small and
  write-rare.
* Give shuli the stored BSSes (not just their channels) so the narrowed
  scan connects to the best *hint-matched* BSS, and let a connected
  roaming-capable BSS trigger a one-shot scan for a better BSS.

## Non-Goals

* Scan-free direct authentication (see
  [Why Not Scan-Free Hints](#why-not-scan-free-hints)).
* BSSID pinning (`wifi-cfg.bssid`), see
  [Future Work](#future-work).

## Design Overview

```
connect success (WifiClientState::run_once, WifiState::Connected*)
    client.export_hints(iface_name) -> bssid + frequency_mhz
    hints::record_bss(ssid, bssid, frequency_mhz)
        skip the write when the BSS is known and today is already recorded
        /var/lib/nipart/wifi/<encoded-ssid>.hints   (0600, atomic rename)

apply (boot hand-off, daemon restart, npt up, ...)
    WifiClientState::apply()
        hints::load_hints(ssid) per configured SSID
            expired file -> deleted, no hints
        build_shuli_network(cfg, hints)
            network.set_hints(frequencies_mhz + known_bss)
    shuli:
        first scan restricted to the hinted channels
        strongest visible known BSS wins (prefered respected)
        no known BSS visible -> one full-scan retry, then normal selection
        after connecting to a roaming-capable BSS -> one-shot better-BSS
        scan, switch when strictly stronger
```

Code map:

| Piece | Location |
|-------|----------|
| Hint file handling | `src/plugin-wifi/hints.rs` |
| Record on connect | `WifiClientState::run_once()`, `src/plugin-wifi/apply.rs` (`save_quick_start_hints()`) |
| Load and attach | `WifiClientState::apply()`, `build_shuli_network()` (`src/plugin-wifi/apply.rs`) |
| Change detection ignoring hints | `same_saved_networks_ignoring_prefered()`, `refresh_hints()` (`src/plugin-wifi/apply.rs`) |
| Delete on profile removal | `hints::remove()` call where `ssids_to_delete` is built (`src/plugin-wifi/apply.rs`) |
| Known-BSS hint type | `KnownBss`, `NetworkConfigHints::known_bss` (shuli `src/lib/config.rs`) |
| Hint-matched selection | `best_hinted_candidate_filtered()`, `process_scan_results()` (shuli `src/lib/scan.rs`) |
| Post-connect best-BSS scan | `arm_post_connect_scan()`, `post_connect_best_bss_scan()`, `connected_wait_duration()` (shuli `src/lib/roam.rs`), connected wait loop (shuli `src/lib/client/wifi_iface.rs`) |

## Hint File

### Location and Name

`/var/lib/nipart/wifi/<encoded-ssid>.hints`.

The directory is created lazily with mode `0700` on the first write. An
SSID is up to 32 UTF-8 bytes and is not a safe path component: `/`,
spaces and a leading `.` are all legal. Every byte outside
`[A-Za-z0-9._-]` is percent-encoded from its UTF-8 bytes, e.g.
`My Wifi/5G` -> `My%20Wifi%2F5G.hints`. The SSID is also stored inside
the file, so a renamed or forged file is discarded on load.

The file is keyed by SSID only, not by phy: the same SSID connected on
several wifi-phys shares one file, and roaming across APs of one ESS
accumulates several BSS entries in it.

### Format

Versioned YAML (`rmsd_yaml`, same conventions as `applied.yml`):

```yaml
version: 1
ssid: My Wifi/5G
updated_on: 20628        # UTC day number (days since 1970-01-01)
bss:
- bssid: 02:00:00:00:00:01
  frequency_mhz: 5180
- bssid: 02:00:00:00:00:02
  frequency_mhz: 2412
```

* `updated_on` is day-granular by design: a reconnect on the same day
  with the same BSS set writes nothing, so a stable single-AP setup
  writes at most once per day.
* `bss` holds at most `MAX_BSS_PER_SSID` (16) entries; the oldest entry
  is dropped when the cap is exceeded.
* The file never contains credentials, RSNE/RSNXE, cipher or signal
  data - only what the quick scan needs.

### Validation

`hints::read_hints()` discards a file (debug log, empty hint set) when:

* the path is not a regular file (symlinks are refused; the file is
  opened with `O_NOFOLLOW`),
* it is larger than 64 KiB,
* it is group- or world-writable,
* its owner differs from its directory's owner (in production the
  directory is root-owned),
* it does not parse as the expected YAML,
* `version` is not the supported one, or the embedded `ssid` does not
  equal the requested SSID.

### Write Path

`hints::record_bss(ssid, bssid, freq_mhz)` is called from
`WifiClientState::run_once()` for every surfaced `Connected*` state
(first connect, reconnect, successful roam) using
`WifiClient::export_hints()` (`shuli`). It:

1. Reads the existing file (if any) unless it is already expired.
2. Upserts the BSS entry by BSSID (a changed frequency updates the
   entry).
3. Caps the list and refreshes `updated_on`.
4. Writes only if the content changed or the day changed; a write is
   skipped otherwise, which is also the steady-state "same BSS, same
   day" no-op.

Errors are logged and swallowed: the connection state is never affected.

### Read Path

`hints::load_frequencies(ssid)` is called once per configured SSID in
`WifiClientState::apply()`. It returns the sorted, deduplicated union of
the stored frequencies. An expired file is deleted here - the SSID is
being applied, which is the deletion point chosen for stale hints.

`build_shuli_network()` attaches the list as
`NetworkConfigHints { frequencies_mhz, ..default }`; an SSID without
hints gets no hint object at all.

### Lifecycle and Cleanup

* Recorded on every new successful BSS (new AP, roam).
* Deleted when the `wifi-cfg` profile is explicitly removed
  (`absent`/`down`) together with its in-memory SSID bookkeeping.
* Kept for `wifi off`, phy absent/down, daemon restart and rollback:
  those do not delete the saved profile.
* Deleted on load when the file is older than `HINT_TTL_DAYS` (30 days).
* `tests/run-tests.sh` snapshots and restores
  `/var/lib/nipart/wifi` around a test run, like it already does for
  `/etc/nipart`, so leftover hints cannot leak between test runs.

### Atomicity

The file is written to `.{name}.hints.tmp.<pid>` (mode `0600`) in the
same directory and then `rename()`d over the old file, so a reader never
sees a torn file. No `fsync`: matching the `applied.yml` conventions, a
hint may be lost on a crash but the next connection records it again.

## Attaching Hints to shuli

nipart attaches the stored frequencies and BSSIDs as
`NetworkConfigHints { frequencies_mhz, known_bss, .. }` on the shuli
`NetworkConfig`. The complete single-BSS scan-free fields (BSSID,
security, RSNE) stay unset, which keeps shuli's scan-free direct-auth
path disabled; the hints drive the quick scan and the hint-matched
selection instead.

### Why Not Scan-Free Hints

shuli enables its scan-free direct-auth path only when a configured
network's hints are complete (`NetworkConfigHints::bss_info()` returns a
`BssInfo`). The quick-start hints deliberately stay incomplete, so shuli
takes the hinted-scan path instead. This was chosen over scan-free auth
for two reasons:

* **Latency safety.** Scan-free auth sends `NL80211_CMD_AUTHENTICATE`
  directly to the stored BSSID and then waits up to
  `AUTH_EVENT_TIMEOUT_SECS` (15 s) for an auth event. On a cold boot the
  BSSID is unknown to cfg80211 and the command fails immediately with
  `-ENOENT`, but when the kernel still caches a stale BSS (daemon
  restart, AP powered off) the client waits the full 15 s before falling
  back. A quick channel scan costs a fraction of that and yields live
  signal data for the selection, so frequency hints are both safer and
  more useful for multi-BSS ESSes.
* **Multi-BSS ordering.** A scan-free attempt can only target one BSS,
  and shuli picks the first network with a complete hint. A hinted scan
  sees every BSS on those channels and picks the best live signal among
  the known ones.

### Hint-Matched Selection

With `known_bss` entries the hinted quick scan works as follows:

* Every configured network contributes its hints: the scan-frequency
  union (`configured_hint_frequencies()`) folds in each network's
  `frequencies_mhz` and `known_bss[].frequency_mhz`, and the selection
  considers the `known_bss` lists of all networks, not just the first
  one.
* The first host scan is restricted to the hinted channels.
* `process_scan_results()` builds the hint-matched candidate set (scan
  results whose BSSID appears in some configured network's `known_bss`)
  and connects to the strongest one within the existing `prefered` tier
  rules. A non-hinted BSS on a hinted channel is never silently
  preferred over a visible known BSS.
* When no known BSS is visible, shuli retries once with a full scan and
  falls back to the normal selection, so a moved AP or a new BSS of the
  same SSID is still found.

`WifiClient::export_hints()` emits the current link as a one-entry
`known_bss` list, so an API user can accumulate the list across
connections exactly the way nipart records it in its hint file.

### Post-Connect Best-BSS Scan

After a fresh connection, shuli arms a one-shot best-BSS scan when the
network has roaming enabled and the connected BSS advertises
managed-roaming support (802.11v BTM or 802.11k neighbor report). The
scan runs `POST_CONNECT_SCAN_DELAY_SECS` (5 s) after the link is up, so
it does not race the DHCP exchange and the first traffic; a
signal-triggered or background roam scan supersedes a still-pending one.
A full scan is used (the connect scan already covered the hinted
channels), and only a strictly stronger BSS of the same ESS is selected;
a different configured SSID only qualifies when the current link is
critical. Unlike the signal-triggered paths, a healthy live signal does
not cancel this scan - moving to a strictly stronger BSS is the point.

One nipart-side subtlety: `NetworkConfig` equality includes `hints`, so
`same_saved_networks_ignoring_prefered()` clears `hints` before
comparing (saved) network lists. Otherwise a BSS recorded while
connected would look like a configuration change and restart the live
shuli client on the next idempotent re-apply. When only the hints
differ, `refresh_hints()` updates the stored list used on the next
client start without touching the running client.

## Security

* Hint files contain no credentials: no password/PSK/PMK, and not even
  RSNE/RSNXE/cipher data. The WIFI password stays in
  `/etc/nipart/applied.secrets.yml`.
* The directory is `0700`, the files `0600`; symlinks are refused, the
  file owner must match the directory owner, group/world-writable files
  are ignored, and parsing is bounded by a 64 KiB size cap.
* Hint content is untrusted input, but it can only narrow or misdirect
  the first scan. shuli still learns the AP's real security from the
  live scan and authenticates against it; hints cannot grant access.
* A local attacker able to write `/var/lib/nipart` is root-equivalent
  already, so no additional privilege boundary is claimed.

## Failure Handling

| Condition | Behavior |
|-----------|----------|
| Hint file missing | Normal full scan. |
| Hint file corrupt / wrong version / SSID mismatch / too large / wrong owner or mode / symlink | Ignored with a debug log; normal full scan. |
| Hints point at channels where nothing configured is found (AP moved channel, powered off) | shuli retries once with a full scan. |
| Hint write fails (disk full, read-only `/var/lib`) | Warn log; the connection is unaffected. |
| File older than 30 days | Ignored and deleted when its SSID is applied. |
| `wifi-cfg` profile removed | Hint file deleted together with the profile. |

## Interaction with the Monitor and Boot Activation

No daemon protocol change is needed. At boot the monitor/event worker
hands the saved `wifi-cfg` profiles to the wifi plugin when a phy
appears, which reaches `WifiClientState::apply()` exactly like a user
apply; the hints are loaded there. The same applies to the rollback
path (`restore_saved_wifi_profiles()`), which rebuilds the plugin's
network list through the same code.

## Testing

Unit tests only, deliberately:

* `src/plugin-wifi/unit_tests/hints.rs` covers SSID encoding/path
  safety, record/load round-trips, the same-day no-write rule, the
  day-granular TTL boundary, BSS cap, upsert/frequency change, corrupt /
  wrong-version / SSID-mismatch / oversized / symlinked / writable
  files, removal idempotence, file permissions and the absence of any
  secret/interface field in the serialized YAML.
* `src/plugin-wifi/unit_tests/apply.rs` covers hint attachment to the
  matching SSID (frequencies and known BSSIDs) and the
  change-detection/refresh behavior.
* The `shuli` crate covers `KnownBss` hint round-trips and the
  known-BSS-only rule, the hint-matched selection
  (`best_hinted_candidate_filtered`: known BSS over a stronger unknown,
  strongest known BSS, `prefered` tiers, none visible) and the
  post-connect scan deadline (`connected_wait_duration`), on top of its
  existing wifi integration suite.

An integration test that asserts "the quick start actually happened" is
intentionally skipped: in the shared test environment an ongoing
workload or a busy channel can make the first scan take longer than a
full scan, so the timing/behavior is not reliably observable and the
test would be flaky rather than meaningful. The scan hint path itself
is covered by shuli's tests.

## Future Work

* **BSSID pinning** (`wifi-cfg.bssid`): the plugin currently drops it;
  shuli has no per-network BSSID pin. Once shuli supports one, a pinned
  profile must not fail over to other APs of the same SSID. Tracked in
  the TODO files of both projects.
* **Scan-free hints**: if direct auth is wanted later, use a short
  (starting at 1 s) hint-auth timeout so a stale BSS falls back quickly.
  A dev experiment (mac80211_hwsim, WPA2-PSK and WPA3-SAE) connected in
  ~130 ms with a 1 s auth-event timeout; real-RF validation is still
  open. Not needed by this design.
* **Proactive roaming on APs without 802.11v/11k**: shuli only runs
  signal-triggered roam scans when the AP advertises BTM (802.11v) or a
  neighbor report (802.11k); other APs are only re-evaluated after a
  disconnect. Out of scope here.
