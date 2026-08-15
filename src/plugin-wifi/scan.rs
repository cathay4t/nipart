// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use nipart::{
    ErrorKind, NipartError, WifiAuthType, WifiAuthTypeDetailed, WifiConfig,
    WifiScanResult,
};
use rtnetlink::packet_core::Parseable;
use wl_nl80211::{
    Ieee80211AkmSuite, Ieee80211CipherSuite, Ieee80211Element,
    Ieee80211Elements,
};

use crate::NipartWpaConn;

impl NipartWpaConn {
    pub(crate) async fn wifi_scan(
        iface_name: Option<&str>,
        show_hidden: bool,
    ) -> Result<Vec<WifiScanResult>, NipartError> {
        if let Ok(r) = _wifi_scan(iface_name, show_hidden).await
            && !r.is_empty()
        {
            return Ok(r);
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        _wifi_scan(iface_name, show_hidden).await
    }
}

async fn _wifi_scan(
    iface_name: Option<&str>,
    show_hidden: bool,
) -> Result<Vec<WifiScanResult>, NipartError> {
    // Keep one entry per SSID, merging auth types from all BSSes of the
    // same SSID and keeping the strongest signal.
    let mut ret: HashMap<String, WifiScanResult> = HashMap::new();

    let mut filter = nispor::NetStateFilter::minimum();
    filter.iface = Some(nispor::NetStateIfaceFilter::minimum());
    let np_state =
        nispor::NetState::retrieve_with_filter_async(&filter).await?;

    let wifi_phys: Vec<&str> = np_state
        .ifaces
        .values()
        .filter_map(|np_iface| {
            if np_iface.iface_type == nispor::IfaceType::Wifi {
                Some(np_iface.name.as_str())
            } else {
                None
            }
        })
        .collect();

    let scan_ifaces = if let Some(iface_name) = iface_name {
        if !wifi_phys.contains(&iface_name) {
            return Err(NipartError::new(
                ErrorKind::InvalidArgument,
                format!("WIFI interface {iface_name} not found"),
            ));
        }
        vec![iface_name]
    } else {
        wifi_phys
    };

    for iface_name in &scan_ifaces {
        let scan_results = shuli::scan::scan_wifi_with_ies(iface_name)
            .await
            .map_err(|e| {
                NipartError::new(
                    ErrorKind::PluginFailure,
                    format!("scan failed on {iface_name}: {e}"),
                )
            })?;

        for (bss_info, ies) in &scan_results {
            if bss_info.hidden && !show_hidden {
                continue;
            }
            let Some(ssid) = extract_ssid(ies) else {
                continue;
            };

            let signal_dbm = signal_mbm_to_dbm(bss_info.signal_dbm);
            let scan_res = WifiScanResult::new(
                ssid.clone(),
                Some(iface_name.to_string()),
                Some(mac_to_string(&bss_info.bssid)),
                Some(bss_info.freq_mhz),
                Some(signal_dbm),
                Some(WifiConfig::signal_dbm_to_percent(signal_dbm)),
                detect_generation(ies),
                vec![detect_auth_type(ies)],
            );

            if let Some(existing) = ret.get_mut(&ssid) {
                // Merge auth types advertised by different BSSes.
                if !existing.auth_types.contains(&scan_res.auth_types[0]) {
                    existing.auth_types.push(scan_res.auth_types[0].clone());
                }
                // Keep the strongest signal per SSID.
                if existing.signal_dbm < scan_res.signal_dbm {
                    existing.base_iface = scan_res.base_iface;
                    existing.bssid = scan_res.bssid;
                    existing.frequency_mhz = scan_res.frequency_mhz;
                    existing.signal_dbm = scan_res.signal_dbm;
                    existing.signal_percent = scan_res.signal_percent;
                    existing.generation = scan_res.generation;
                }
            } else {
                ret.insert(ssid, scan_res);
            }
        }
    }

    let mut ret: Vec<WifiScanResult> = ret.into_values().collect();
    // Sort by signal strength (strongest first), then SSID for a
    // deterministic output order.
    ret.sort_unstable_by(|a, b| {
        b.signal_percent
            .cmp(&a.signal_percent)
            .then_with(|| a.ssid.cmp(&b.ssid))
    });
    Ok(ret)
}

fn extract_ssid(ies: &[u8]) -> Option<String> {
    let mut pos = 0;
    while pos + 2 <= ies.len() {
        let id = ies[pos];
        let len = ies[pos + 1] as usize;
        if id == 0 && pos + 2 + len <= ies.len() {
            return String::from_utf8(ies[pos + 2..pos + 2 + len].to_vec())
                .ok();
        }
        pos += 2 + len;
    }
    None
}

/// Detect the detailed auth type of the AP from its RSNE(Robust Security
/// Network Element). Returns `OPEN` when the network has no RSNE.
fn detect_auth_type(ies: &[u8]) -> WifiAuthTypeDetailed {
    let Ok(parsed) = Ieee80211Elements::parse(ies) else {
        return open_auth_type();
    };
    let elems = parsed.0;
    let rsn = elems.iter().find_map(|ie| match ie {
        Ieee80211Element::Rsn(rsn) => Some(rsn),
        _ => None,
    });
    let Some(rsn) = rsn else {
        // No RSNE: either an open network, or legacy security(WPA1/WEP)
        // which has no simplified `WifiAuthType`. Detect the WPA1 vendor
        // IE so such networks are not mislabeled as `OPEN`.
        if elems.iter().any(|ie| match ie {
            Ieee80211Element::Vendor(payload) => is_wpa1_vendor_ie(payload),
            _ => false,
        }) {
            // WPA1 is deprecated and has no simplified `WifiAuthType`.
            return WifiAuthTypeDetailed::default();
        }
        return open_auth_type();
    };

    let mut cipher = Vec::new();
    if let Some(group_cipher) = rsn.group_cipher {
        cipher.push(cipher_to_string(group_cipher));
    }
    for pairwise_cipher in &rsn.pairwise_ciphers {
        let c = cipher_to_string(*pairwise_cipher);
        if !cipher.contains(&c) {
            cipher.push(c);
        }
    }

    WifiAuthTypeDetailed::new(
        auth_type_from_akm(&rsn.akm_suits),
        rsn.akm_suits
            .iter()
            .map(|akm| akm_to_string(*akm))
            .collect(),
        cipher,
    )
}

fn open_auth_type() -> WifiAuthTypeDetailed {
    WifiAuthTypeDetailed::new(WifiAuthType::Open, Vec::new(), Vec::new())
}

/// WPA IE: vendor-specific element with OUI 00:50:F2 (Microsoft) and
/// OUI type 1 (WPA). Used by WPA1, which has no RSNE.
fn is_wpa1_vendor_ie(payload: &[u8]) -> bool {
    payload.len() >= 4
        && payload[0] == 0x00
        && payload[1] == 0x50
        && payload[2] == 0xf2
        && payload[3] == 0x01
}

/// Map the AKM suites advertised by the AP to the simplified auth type.
fn auth_type_from_akm(akm_suits: &[Ieee80211AkmSuite]) -> WifiAuthType {
    if akm_suits.iter().any(|akm| {
        matches!(
            akm,
            Ieee80211AkmSuite::Sae
                | Ieee80211AkmSuite::FtSae
                | Ieee80211AkmSuite::SaeGroupDependentHash
                | Ieee80211AkmSuite::FtSaeGroupDependentHash
        )
    }) {
        WifiAuthType::Wpa3Personal
    } else if akm_suits.iter().any(|akm| {
        matches!(
            akm,
            Ieee80211AkmSuite::Psk
                | Ieee80211AkmSuite::FtPsk
                | Ieee80211AkmSuite::PskSha256
                | Ieee80211AkmSuite::PskSha384
                | Ieee80211AkmSuite::FtPskSha384
        )
    }) {
        WifiAuthType::Wpa2Personal
    } else {
        // EAP(Enterprise) networks are not supported yet, report as Unknown.
        WifiAuthType::Unknown
    }
}

fn akm_to_string(akm: Ieee80211AkmSuite) -> String {
    match akm {
        Ieee80211AkmSuite::Ieee8021x => "802.1X".into(),
        Ieee80211AkmSuite::Psk => "PSK".into(),
        Ieee80211AkmSuite::FtIeee8021x => "FT-802.1X".into(),
        Ieee80211AkmSuite::FtPsk => "FT-PSK".into(),
        Ieee80211AkmSuite::Ieee8021xSha256 => "802.1X-SHA256".into(),
        Ieee80211AkmSuite::PskSha256 => "PSK-SHA256".into(),
        Ieee80211AkmSuite::Tdls => "TDLS".into(),
        Ieee80211AkmSuite::Sae => "SAE".into(),
        Ieee80211AkmSuite::FtSae => "FT-SAE".into(),
        Ieee80211AkmSuite::ApPeerKey => "AP-PEER-KEY".into(),
        Ieee80211AkmSuite::Ieee8021xSuiteB => "802.1X-SUITE-B".into(),
        Ieee80211AkmSuite::Ieee8021xCnsa => "802.1X-CNSA".into(),
        Ieee80211AkmSuite::FtIeee8021xSha384 => "FT-802.1X-SHA384".into(),
        Ieee80211AkmSuite::FilsSha256AesSiv256OrIeee8021x => {
            "FILS-SHA256".into()
        }
        Ieee80211AkmSuite::FilsSha384AesSiv512OrIeee8021x => {
            "FILS-SHA384".into()
        }
        Ieee80211AkmSuite::FtFilsSha256AesSiv256OrIeee8021x => {
            "FT-FILS-SHA256".into()
        }
        Ieee80211AkmSuite::FtFilsSha384AesSiv512OrIeee8021x => {
            "FT-FILS-SHA384".into()
        }
        Ieee80211AkmSuite::Owe => "OWE".into(),
        Ieee80211AkmSuite::FtPskSha384 => "FT-PSK-SHA384".into(),
        Ieee80211AkmSuite::PskSha384 => "PSK-SHA384".into(),
        Ieee80211AkmSuite::SaeGroupDependentHash => "SAE-GROUP-HASH".into(),
        Ieee80211AkmSuite::FtSaeGroupDependentHash => {
            "FT-SAE-GROUP-HASH".into()
        }
        Ieee80211AkmSuite::Other(d) => format!("0x{d:08x}"),
        _ => "UNKNOWN".into(),
    }
}

fn cipher_to_string(cipher: Ieee80211CipherSuite) -> String {
    match cipher {
        Ieee80211CipherSuite::UseGroup => "USE-GROUP".into(),
        Ieee80211CipherSuite::Wep40 => "WEP-40".into(),
        Ieee80211CipherSuite::Tkip => "TKIP".into(),
        Ieee80211CipherSuite::Ccmp128 => "CCMP".into(),
        Ieee80211CipherSuite::Wep104 => "WEP-104".into(),
        Ieee80211CipherSuite::BipCmac128 => "BIP-CMAC-128".into(),
        Ieee80211CipherSuite::GroupAddressedTrafficNotAllowed => {
            "GAT-NOT-ALLOWED".into()
        }
        Ieee80211CipherSuite::Gcmp128 => "GCMP".into(),
        Ieee80211CipherSuite::Gcmp256 => "GCMP-256".into(),
        Ieee80211CipherSuite::Ccmp256 => "CCMP-256".into(),
        Ieee80211CipherSuite::BipGmac128 => "BIP-GMAC-128".into(),
        Ieee80211CipherSuite::BipGmac256 => "BIP-GMAC-256".into(),
        Ieee80211CipherSuite::BipCmac256 => "BIP-CMAC-256".into(),
        Ieee80211CipherSuite::Other(d) => format!("0x{d:08x}"),
        _ => "UNKNOWN".into(),
    }
}

fn detect_generation(ies: &[u8]) -> Option<u32> {
    if let Ok(parsed) = Ieee80211Elements::parse(ies) {
        let elems = parsed.0;
        if elems
            .iter()
            .any(|ie| matches!(ie, Ieee80211Element::HeCapability(_)))
        {
            return Some(6);
        } else if elems
            .iter()
            .any(|ie| matches!(ie, Ieee80211Element::VhtCapability(_)))
        {
            return Some(5);
        } else if elems
            .iter()
            .any(|ie| matches!(ie, Ieee80211Element::HtCapability(_)))
        {
            return Some(4);
        }
    }
    None
}

fn mac_to_string(mac: &[u8; 6]) -> String {
    mac.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Convert the nl80211 scan signal from mBm to dBm. `-3000` means `-30 dBm`.
fn signal_mbm_to_dbm(signal_mbm: i32) -> i16 {
    (signal_mbm / 100) as i16
}

#[cfg(test)]
mod test {
    use super::*;

    /// Build a valid RSNE(Robust Security Network Element) with group/pairwise
    /// cipher CCMP and the given AKM suites.
    fn rsn_ie(akm: &[u8]) -> Vec<u8> {
        let mut payload = vec![0x01, 0x00]; // version
        payload.extend_from_slice(&[0x00, 0x0f, 0xac, 0x04]); // group CCMP
        payload.extend_from_slice(&[0x01, 0x00]); // pairwise count
        payload.extend_from_slice(&[0x00, 0x0f, 0xac, 0x04]); // pairwise CCMP
        payload.extend_from_slice(&[(akm.len() / 4) as u8, 0x00]); // akm count
        payload.extend_from_slice(akm);
        payload.extend_from_slice(&[0x00, 0x00]); // rsn capabilities
        let mut ret = vec![0x30, payload.len() as u8];
        ret.extend_from_slice(&payload);
        ret
    }

    #[test]
    fn test_detect_auth_type_wpa2_psk() {
        let auth = detect_auth_type(&rsn_ie(&[0x00, 0x0f, 0xac, 0x02]));
        assert_eq!(auth.auth_type, WifiAuthType::Wpa2Personal);
        assert_eq!(auth.akm, vec!["PSK"]);
        assert_eq!(auth.cipher, vec!["CCMP"]);
    }

    #[test]
    fn test_detect_auth_type_wpa3_sae() {
        let auth = detect_auth_type(&rsn_ie(&[0x00, 0x0f, 0xac, 0x08]));
        assert_eq!(auth.auth_type, WifiAuthType::Wpa3Personal);
        assert_eq!(auth.akm, vec!["SAE"]);
        assert_eq!(auth.cipher, vec!["CCMP"]);
    }

    #[test]
    fn test_detect_auth_type_transition_mode_prefers_sae() {
        // WPA2/WPA3 transition mode: PSK + SAE.
        let auth = detect_auth_type(&rsn_ie(&[
            0x00, 0x0f, 0xac, 0x02, 0x00, 0x0f, 0xac, 0x08,
        ]));
        assert_eq!(auth.auth_type, WifiAuthType::Wpa3Personal);
        assert_eq!(auth.akm, vec!["PSK", "SAE"]);
    }

    #[test]
    fn test_detect_auth_type_eap_is_unknown() {
        // EAP(Enterprise) is not supported yet, report as Unknown. The AKM
        // details are still listed in the detailed result.
        let auth = detect_auth_type(&rsn_ie(&[0x00, 0x0f, 0xac, 0x01]));
        assert_eq!(auth.auth_type, WifiAuthType::Unknown);
        assert_eq!(auth.akm, vec!["802.1X"]);
        // Suite B(802.1X-SHA384) too.
        let auth = detect_auth_type(&rsn_ie(&[0x00, 0x0f, 0xac, 0x0b]));
        assert_eq!(auth.auth_type, WifiAuthType::Unknown);
        assert_eq!(auth.akm, vec!["802.1X-SUITE-B"]);
    }

    #[test]
    fn test_detect_auth_type_open_without_rsne() {
        // SSID element only: open network.
        let auth =
            detect_auth_type(&[0x00, 0x05, b'h', b'e', b'l', b'l', b'o']);
        assert_eq!(auth.auth_type, WifiAuthType::Open);
        assert!(auth.akm.is_empty());
        assert!(auth.cipher.is_empty());
    }

    #[test]
    fn test_detect_auth_type_wpa1_vendor_ie() {
        // WPA IE: vendor element with OUI 00:50:F2 and OUI type 1.
        let wpa_ie =
            [0xdd, 0x08, 0x00, 0x50, 0xf2, 0x01, 0x01, 0x00, 0x00, 0x50];
        let auth = detect_auth_type(&wpa_ie);
        assert_eq!(auth.auth_type, WifiAuthType::Unknown);
    }

    #[test]
    fn test_signal_mbm_to_dbm() {
        assert_eq!(signal_mbm_to_dbm(-3000), -30);
        assert_eq!(signal_mbm_to_dbm(-4500), -45);
        assert_eq!(signal_mbm_to_dbm(-6500), -65);
    }
}
