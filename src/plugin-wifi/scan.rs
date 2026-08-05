// SPDX-License-Identifier: Apache-2.0

use nipart::{ErrorKind, NipartError, WifiAuthType, WifiConfig};
use rtnetlink::packet_core::Parseable;
use wl_nl80211::{Nl80211Element, Nl80211Elements};

use crate::NipartWpaConn;

impl NipartWpaConn {
    pub(crate) async fn wifi_scan(
        iface_name: Option<&str>,
    ) -> Result<Vec<WifiConfig>, NipartError> {
        if let Ok(r) = _wifi_scan(iface_name).await
            && !r.is_empty()
        {
            return Ok(r);
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        _wifi_scan(iface_name).await
    }
}

async fn _wifi_scan(
    iface_name: Option<&str>,
) -> Result<Vec<WifiConfig>, NipartError> {
    let mut ret = Vec::new();

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
            let Some(ssid) = extract_ssid(ies) else {
                continue;
            };

            let wifi_cfg = WifiConfig {
                ssid,
                base_iface: Some(iface_name.to_string()),
                bssid: Some(mac_to_string(&bss_info.bssid)),
                frequency_mhz: Some(bss_info.freq_mhz),
                signal_dbm: Some(bss_info.signal_dbm as i16),
                auth_types: Some(security_to_auth_types(bss_info.security)),
                generation: detect_generation(ies),
                ..Default::default()
            };

            // Keep strongest signal per SSID.
            if let Some(existing) = ret
                .iter_mut()
                .find(|w: &&mut WifiConfig| w.ssid == wifi_cfg.ssid)
            {
                if existing.signal_dbm < wifi_cfg.signal_dbm {
                    *existing = wifi_cfg;
                }
            } else {
                ret.push(wifi_cfg);
            }
        }
    }

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

fn security_to_auth_types(security: shuli::SecurityType) -> Vec<WifiAuthType> {
    match security {
        shuli::SecurityType::Open => vec![WifiAuthType::Open],
        shuli::SecurityType::Wpa2Psk => vec![WifiAuthType::Wpa2Personal],
        shuli::SecurityType::Owe => vec![WifiAuthType::Wpa3Open],
        shuli::SecurityType::Sae => vec![WifiAuthType::Wpa3Personal],
    }
}

fn detect_generation(ies: &[u8]) -> Option<u32> {
    if let Ok(parsed) = Nl80211Elements::parse(ies) {
        let elems = parsed.0;
        if elems
            .iter()
            .any(|ie| matches!(ie, Nl80211Element::HeCapability(_)))
        {
            return Some(6);
        } else if elems
            .iter()
            .any(|ie| matches!(ie, Nl80211Element::VhtCapability(_)))
        {
            return Some(5);
        } else if elems
            .iter()
            .any(|ie| matches!(ie, Nl80211Element::HtCapability(_)))
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
