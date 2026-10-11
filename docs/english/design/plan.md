<!-- vim-markdown-toc GFM -->

* [Quick Boot Plan](#quick-boot-plan)
    * [Motivation](#motivation)
    * [WIFI Quick Boot](#wifi-quick-boot)
    * [DHCP Lease Quick Boot](#dhcp-lease-quick-boot)
    * [File Format and Lifecycle](#file-format-and-lifecycle)
    * [Security and Correctness](#security-and-correctness)
    * [Open Items](#open-items)

<!-- vim-markdown-toc -->

# Quick Boot Plan

Future plan, not yet implemented. This document records the intended
design for shortening daemon-start and OS-boot network setup by reusing
information from the previous run. It complements the interface monitor
design in [monitor.md](monitor.md): quick boot is an optimization, the
monitor's event/reconciliation path stays the correctness authority.

## Motivation

A cold WIFI boot costs a full channel scan, SAE/4-way authentication and
a DHCP DISCOVER. After a daemon restart or reboot much of that
information is still known: the last connected BSS, its channel and
security, the last DHCP lease. Feeding it back lets the wifi plugin skip
(or narrow) the scan and lets the DHCP client renew the known address
instead of starting from DISCOVER.

## WIFI Quick Boot

Implemented as the frequency-hint quick start; see
[wifi_quick_start.md](wifi_quick_start.md) for the design and the code
layout. The earlier sketch below - persisting complete
`NetworkConfigHints` (BSSID/security/RSNE) to a single
`/var/lib/nipart/wifi_quick_boot.yml` for scan-free authentication - was
superseded: nipart now stores BSSID + frequency per SSID under
`/var/lib/nipart/wifi/` and attaches frequency-only hints, which keeps
shuli's scan-free path disabled and bounds stale-hint latency to one
quick scan.

## DHCP Lease Quick Boot

Persist the last active lease per interface (address and prefix length,
gateway, DNS servers, server identifier, lease time/expiry, client ID or
DUID). The exact file name is open; `/var/lib/nipart/dhcp_quick_boot.yml`
mirrors the implemented WIFI quick-start layout
([wifi_quick_start.md](wifi_quick_start.md)).

On daemon start:

1. Apply the persisted address to the interface as a tentative address so
   connectivity returns before the DHCP exchange completes.
2. Start the DHCP client immediately with the persisted lease
   (INIT-REBOOT/REQUEST using the known address/AID), replacing the
   tentative state when the server answers.

The tentative address is a hint, not a guarantee: a stale lease must be
dropped and a full DISCOVER issued when the server NAKs, the lease
expired, or the interface's saved config changed.

## File Format and Lifecycle

These bullets apply to the future DHCP quick-boot file; the WIFI hints
are implemented with a different per-SSID layout, see
[wifi_quick_start.md](wifi_quick_start.md).

* Versioned YAML, root-only (`0600`), following the `applied.yml`
  conventions.
* Written after a successful association/lease and refreshed
  periodically; removed for interfaces whose saved config was removed.
* Best effort: any read/validation error discards the file and normal
  scan/DISCOVER boot proceeds.
* Hints are only used for the interface and SSID they were recorded for;
  a changed saved config invalidates them.

## Security and Correctness

* No credentials in quick-boot files.
* Treat the file as untrusted input: verify security/BSSID before use and
  reconcile with the kernel state, the monitor events and the DHCP
  server's answer.
* The tentative DHCP address must be short-lived and never survive a NAK.

## Open Items

* Persist PMKSA / 802.11r state to skip the full SAE exchange? Sensitive,
  needs a threat model and key protection. The WIFI quick start
  deliberately does not do this.
* Roaming hints for multiple BSSIDs of the same ESS - implemented for
  WIFI by accumulating every connected BSS in one per-SSID hint file.
* Interaction with `wait-online` and the daemon online latch.
* Whether WIFI and DHCP information share one quick-boot file or stay in
  separate ones - decided: separate per-SSID WIFI files and a separate
  DHCP file.
