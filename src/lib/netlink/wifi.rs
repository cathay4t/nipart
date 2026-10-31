// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nispor project
// (https://github.com/nispor/nispor) which is under license of Apache 2.0.

use std::collections::HashMap;

use futures_util::TryStreamExt;
use wl_nl80211::{
    Ieee80211Element, Ieee80211Elements, Nl80211Attr, Nl80211BssInfo,
    Nl80211Handle, Nl80211InterfaceType, Nl80211RateInfo, Nl80211StationFlags,
    Nl80211StationInfo, packet_core::Parseable,
};

use crate::{WifiConfig, netlink::mac::format_mac_address};

pub(crate) async fn get_wifi_infos() -> HashMap<String, Option<WifiConfig>> {
    let mut ret = HashMap::new();
    let (connection, handle, _) = match wl_nl80211::new_connection() {
        Ok(value) => value,
        Err(error) => {
            log::warn!("Failed to create nl80211 netlink connection: {error}");
            return ret;
        }
    };
    tokio::spawn(connection);

    let mut wifi_ifaces: HashMap<String, WifiConfig> = HashMap::new();
    let mut station_ifaces: Vec<(String, u32)> = Vec::new();
    let mut interface_stream =
        handle.interface().get(Vec::new()).execute().await;
    while let Some(message) = match interface_stream.try_next().await {
        Ok(message) => message,
        Err(error) => {
            log::warn!("Failed to query nl80211 interfaces: {error}");
            return ret;
        }
    } {
        let attributes = &message.payload.attributes;
        let Some(iface_name) = attributes.iter().find_map(|attribute| {
            if let Nl80211Attr::IfName(name) = attribute {
                Some(name.to_string())
            } else {
                None
            }
        }) else {
            continue;
        };
        let mut config = WifiConfig::default();
        let mut mode = Nl80211InterfaceType::Unspecified;
        let mut iface_index = None;
        for attribute in attributes {
            match attribute {
                Nl80211Attr::WiphyFreq(frequency) => {
                    config.frequency_mhz = Some(*frequency)
                }
                Nl80211Attr::Ssid(ssid) if !ssid.is_empty() => {
                    config.ssid = ssid.to_string()
                }
                Nl80211Attr::SsidRaw(ssid) if !ssid.is_empty() => {
                    config.ssid = String::from_utf8_lossy(ssid).to_string()
                }
                Nl80211Attr::IfType(iface_type) => mode = *iface_type,
                Nl80211Attr::IfIndex(index) => iface_index = Some(*index),
                _ => (),
            }
        }
        if mode == Nl80211InterfaceType::Ap
            && let Some(Nl80211Attr::Mac(mac)) = attributes
                .iter()
                .find(|attribute| matches!(attribute, Nl80211Attr::Mac(_)))
        {
            config.bssid = format_mac_address(mac.as_slice())
                .map(|mac| mac.to_uppercase());
        }
        if mode == Nl80211InterfaceType::Station
            && let Some(iface_index) = iface_index
        {
            station_ifaces.push((iface_name.clone(), iface_index));
        }
        wifi_ifaces.insert(iface_name, config);
    }

    for (iface_name, iface_index) in station_ifaces {
        let Some(config) = wifi_ifaces.get_mut(&iface_name) else {
            continue;
        };
        let mut authorized = false;
        let mut mac_to_ssid = HashMap::new();
        if config.ssid.is_empty()
            && let Ok(map) = get_mac_ssid_map(&handle, iface_index).await
        {
            mac_to_ssid = map;
        }
        let mut station_stream =
            handle.station().dump(iface_index).execute().await;
        loop {
            let message = match station_stream.try_next().await {
                Ok(Some(message)) => message,
                Ok(None) => break,
                Err(error) => {
                    log::debug!(
                        "Failed to query nl80211 station of {iface_name}: \
                         {error}"
                    );
                    break;
                }
            };
            let attributes = &message.payload.attributes;
            if let Some(station_mac) = attributes.iter().find_map(|attribute| {
                if let Nl80211Attr::Mac(mac) = attribute {
                    Some(mac)
                } else {
                    None
                }
            }) && let Some(mac) = format_mac_address(station_mac.as_slice())
            {
                if let Some(ssid) = mac_to_ssid.get(&mac) {
                    config.ssid = ssid.to_string();
                }
                config.bssid = Some(mac.to_uppercase());
            }
            let station_infos = attributes.iter().find_map(|attribute| {
                if let Nl80211Attr::StationInfo(infos) = attribute {
                    Some(infos)
                } else {
                    None
                }
            });
            let Some(station_infos) = station_infos else {
                continue;
            };
            for info in station_infos {
                match info {
                    Nl80211StationInfo::Signal(signal) => {
                        config.signal_dbm = Some(*signal as i16)
                    }
                    Nl80211StationInfo::TxBitrate(rates) => {
                        for rate in rates {
                            if let Nl80211RateInfo::Bitrate32(bitrate) = rate {
                                config.tx_bitrate_mb = Some(*bitrate / 10);
                            }
                        }
                    }
                    Nl80211StationInfo::RxBitrate(rates) => {
                        for rate in rates {
                            match rate {
                                Nl80211RateInfo::Bitrate32(bitrate) => {
                                    config.rx_bitrate_mb = Some(*bitrate / 10)
                                }
                                Nl80211RateInfo::Mcs(_) => {
                                    config.generation = Some(4)
                                }
                                Nl80211RateInfo::VhtMcs(_) => {
                                    config.generation = Some(5)
                                }
                                Nl80211RateInfo::HeMcs(_) => {
                                    config.generation = Some(6)
                                }
                                Nl80211RateInfo::EhtMcs(_) => {
                                    config.generation = Some(7)
                                }
                                _ => (),
                            }
                        }
                    }
                    Nl80211StationInfo::StationFlags(flags)
                        if flags
                            .set
                            .contains(Nl80211StationFlags::Authorized) =>
                    {
                        authorized = true;
                    }
                    _ => (),
                }
            }
        }
        // TODO: Once wl_nl80211 supports NL80211_ATTR_AUTH_TYPE, only clear
        // the SSID for WEP or open system connections.
        if !authorized {
            config.ssid.clear();
        }
    }

    for (iface_name, mut config) in wifi_ifaces {
        config.sanitize_signal();
        if config.ssid.is_empty() {
            ret.insert(iface_name, None);
        } else {
            ret.insert(iface_name, Some(config));
        }
    }
    ret
}

