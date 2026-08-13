// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashSet,
    io::{IsTerminal, Write, stdin, stdout},
};

use nipart::{
    Interface, NetworkState, NipartClient, NipartQueryOption,
    NipartWifiScanOption, WifiAuthType, WifiScanResult,
};
use nix::sys::termios::{LocalFlags, SetArg, tcgetattr, tcsetattr};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::CliError;

const WIFI_TABLE_HEADERS: [&str; 8] = [
    "IN-USE", "BSSID", "SSID", "CHAN", "BAND", "SIGNAL", "BARS", "SECURITY",
];
const WIFI_SSID_MIN_WIDTH: usize = 16;
const COLOR_GREEN: &str = "\x1b[32m";
const COLOR_YELLOW: &str = "\x1b[33m";
const COLOR_MAGENTA: &str = "\x1b[35m";
const COLOR_CYAN: &str = "\x1b[36m";
const COLOR_DIM: &str = "\x1b[2m";
const COLOR_CLEAR: &str = "\x1b[0m";

pub(crate) struct CommandWifi;

impl CommandWifi {
    pub(crate) const CMD: &str = "wifi";

    pub(crate) fn new_cmd() -> clap::Command {
        clap::Command::new("wifi")
            .about("WIFI actions")
            .subcommand_required(true)
            .subcommand(
                clap::Command::new("scan")
                    .about("WIFI active scan")
                    .alias("s")
                    .arg(
                        clap::Arg::new("IFACE")
                            .required(false)
                            .index(1)
                            .help("Scan on specified interface only"),
                    )
                    .arg(
                        clap::Arg::new("YAML")
                            .short('y')
                            .long("yaml")
                            .action(clap::ArgAction::SetTrue)
                            .help("Show scan result in YAML format"),
                    ),
            )
            .subcommand(
                clap::Command::new("connect")
                    .alias("c")
                    .about("Connect WIFI")
                    .arg(
                        clap::Arg::new("SSID")
                            .required(true)
                            .index(1)
                            .help("SSID to connect"),
                    )
                    .arg(
                        clap::Arg::new("NO_PASS")
                            .long("no-pass")
                            .action(clap::ArgAction::SetTrue)
                            .help(
                                "Do not ask for password(SSID does not \
                                 require password to connect)",
                            ),
                    ),
            )
    }

    pub(crate) async fn handle(
        matches: &clap::ArgMatches,
    ) -> Result<(), CliError> {
        if let Some(matches) = matches.subcommand_matches("scan") {
            let mut cli = NipartClient::new().await?;
            let mut opt = NipartWifiScanOption::default();
            opt.iface_name = matches.get_one::<String>("IFACE").cloned();
            let active_ssids = match cli
                .query_network_state(NipartQueryOption::running())
                .await
            {
                Ok(net_state) => collect_active_ssids(&net_state),
                Err(e) => {
                    log::warn!(
                        "Failed to query network state for active SSID \
                         markers: {e}"
                    );
                    HashSet::new()
                }
            };
            let mut wifi_cfgs = cli.wifi_scan(opt).await?;
            wifi_cfgs.sort_unstable_by_key(|wifi_cfg| wifi_cfg.signal_percent);
            wifi_cfgs.reverse();
            if matches.get_flag("YAML") {
                println!("{}", rmsd_yaml::to_string(&wifi_cfgs)?);
            } else {
                let table = wifi_scan_table(&wifi_cfgs, &active_ssids);
                print!(
                    "{}",
                    colorize_wifi_scan_table(
                        &table,
                        &wifi_cfgs,
                        color_enabled(),
                    )
                );
            }
        } else if let Some(matches) = matches.subcommand_matches("connect") {
            // It is safe to unwrap because of clap `required: true`
            let ssid = matches.get_one::<String>("SSID").unwrap();
            let state_str = if matches.get_flag("NO_PASS") {
                format!(
                    r#"---
                    interfaces:
                    - name: {ssid}
                      type: wifi-cfg
                      state: up
                      ipv4:
                        enabled: true
                        dhcp: true
                      wifi:
                        ssid: {ssid}
                    "#
                )
            } else {
                let pass = getpass()?;
                format!(
                    r#"---
                    interfaces:
                    - name: {ssid}
                      type: wifi-cfg
                      state: up
                      ipv4:
                        enabled: true
                        dhcp: true
                      wifi:
                        ssid: {ssid}
                        password: {pass}
                    "#
                )
            };

            let desired_state: nipart::NetworkState =
                rmsd_yaml::from_str(&state_str)?;
            let mut desired_state_to_show = desired_state.clone();
            desired_state_to_show.hide_secrets();
            log::info!(
                "Applying desire state:\n{}",
                rmsd_yaml::to_string(&desired_state_to_show)?
            );
            let mut cli = NipartClient::new().await?;
            cli.apply_network_state(desired_state, Default::default())
                .await?;
        }
        Ok(())
    }
}

/// Build an `nmcli device wifi list`-style table from scan results.
fn wifi_scan_table(
    wifi_cfgs: &[WifiScanResult],
    active_ssids: &HashSet<String>,
) -> String {
    let rows: Vec<[String; WIFI_TABLE_HEADERS.len()]> = wifi_cfgs
        .iter()
        .map(|wifi_cfg| {
            let in_use = if active_ssids.contains(&wifi_cfg.ssid) {
                "*"
            } else {
                ""
            };
            let channel = wifi_cfg
                .frequency_mhz
                .and_then(freq_to_channel)
                .map(|c| c.to_string())
                .unwrap_or_else(|| "--".to_string());
            let band = wifi_cfg
                .frequency_mhz
                .and_then(freq_to_band)
                .unwrap_or("--")
                .to_string();
            let signal = wifi_cfg
                .signal_percent
                .map(|s| s.to_string())
                .unwrap_or_else(|| "--".to_string());
            let bars = wifi_cfg
                .signal_percent
                .map(wifi_strength_bars)
                .unwrap_or("____");
            let ssid = display_ssid(&wifi_cfg.ssid);
            [
                in_use.to_string(),
                wifi_cfg.bssid.as_deref().unwrap_or("--").to_uppercase(),
                ssid,
                channel,
                band,
                signal,
                bars.to_string(),
                security_string(&wifi_cfg.auth_types),
            ]
        })
        .collect();

    let mut widths = WIFI_TABLE_HEADERS
        .iter()
        .map(|header| header.len())
        .collect::<Vec<_>>();
    for row in &rows {
        for (idx, value) in row.iter().enumerate() {
            widths[idx] = widths[idx].max(value.width());
        }
    }
    widths[2] = widths[2].max(WIFI_SSID_MIN_WIDTH);

    let mut ret = String::new();
    append_table_row(
        &mut ret,
        &WIFI_TABLE_HEADERS
            .iter()
            .map(|header| (*header).to_string())
            .collect::<Vec<_>>(),
        &widths,
    );
    for row in &rows {
        append_table_row(&mut ret, row, &widths);
    }
    ret
}

fn append_table_row(ret: &mut String, row: &[String], widths: &[usize]) {
    for (idx, value) in row.iter().enumerate() {
        if idx + 1 < row.len() {
            ret.push_str(&pad_to_width(value, widths[idx] + 1));
            ret.push(' ');
        } else {
            ret.push_str(value);
        }
    }
    ret.push('\n');
}

fn pad_to_width(value: &str, width: usize) -> String {
    let padding = width.saturating_sub(value.width());
    format!("{value}{}", " ".repeat(padding))
}

/// Render an SSID for the table. Control characters and zero-width
/// characters are replaced with `?` so the terminal display width matches
/// the calculated width; SSIDs without any visible characters (hidden
/// networks) are shown as `--`.
fn display_ssid(ssid: &str) -> String {
    let mut visible = String::new();
    let mut ret = String::new();
    for c in ssid.chars() {
        if c.is_control() || c.width() == Some(0) {
            ret.push('?');
        } else {
            ret.push(c);
            if !c.is_whitespace() {
                visible.push(c);
            }
        }
    }
    if visible.is_empty() {
        "--".to_string()
    } else {
        ret
    }
}

fn collect_active_ssids(net_state: &NetworkState) -> HashSet<String> {
    let mut ret = HashSet::new();
    for iface in net_state.ifaces.iter() {
        if let Interface::WifiPhy(wifi_phy) = iface
            && let Some(wifi_cfg) = wifi_phy.wifi.as_ref()
            && !wifi_cfg.ssid.is_empty()
        {
            ret.insert(wifi_cfg.ssid.clone());
        }
    }
    ret
}

fn freq_to_channel(freq: u32) -> Option<u32> {
    if (2412..=2472).contains(&freq) && (freq - 2412).is_multiple_of(5) {
        Some((freq - 2407) / 5)
    } else if freq == 2484 {
        Some(14)
    } else if (4915..=4980).contains(&freq) && (freq - 4915).is_multiple_of(5) {
        Some((freq - 4000) / 5)
    } else if (5160..=5825).contains(&freq) && (freq - 5160).is_multiple_of(5) {
        Some((freq - 5000) / 5)
    } else if (5955..=7115).contains(&freq) && (freq - 5955).is_multiple_of(5) {
        Some((freq - 5950) / 5)
    } else {
        None
    }
}

fn freq_to_band(freq: u32) -> Option<&'static str> {
    if (2412..=2484).contains(&freq) {
        Some("2.4 GHz")
    } else if (4915..=5825).contains(&freq) {
        Some("5 GHz")
    } else if (5955..=7115).contains(&freq) {
        Some("6 GHz")
    } else {
        None
    }
}

fn wifi_strength_bars(strength: u8) -> &'static str {
    if strength > 80 {
        "▂▄▆█"
    } else if strength > 55 {
        "▂▄▆_"
    } else if strength > 30 {
        "▂▄__"
    } else if strength > 5 {
        "▂___"
    } else {
        "____"
    }
}

fn color_enabled() -> bool {
    if !stdout().is_terminal() {
        return false;
    }
    !matches!(
        std::env::var("NO_COLOR"),
        Ok(value) if !value.is_empty()
    )
}

fn colorize_wifi_scan_table(
    table: &str,
    wifi_cfgs: &[WifiScanResult],
    enabled: bool,
) -> String {
    if !enabled {
        return table.to_string();
    }

    let mut lines = table.lines();
    let mut ret = String::new();
    if let Some(header) = lines.next() {
        ret.push_str(header);
        ret.push('\n');
    }
    for (wifi_cfg, line) in wifi_cfgs.iter().zip(lines) {
        ret.push_str(wifi_signal_color(wifi_cfg.signal_percent));
        ret.push_str(line);
        ret.push_str(COLOR_CLEAR);
        ret.push('\n');
    }
    ret
}

fn wifi_signal_color(signal_percent: Option<u8>) -> &'static str {
    match signal_percent {
        Some(s) if s > 80 => COLOR_GREEN,
        Some(s) if s > 55 => COLOR_YELLOW,
        Some(s) if s > 30 => COLOR_MAGENTA,
        Some(s) if s > 5 => COLOR_CYAN,
        _ => COLOR_DIM,
    }
}

fn security_string(auth_types: &[nipart::WifiAuthTypeDetailed]) -> String {
    let mut labels = Vec::new();
    for auth_type in auth_types {
        let label = match auth_type.auth_type {
            WifiAuthType::Open => "OPEN",
            WifiAuthType::Wpa2Personal => "WPA2",
            WifiAuthType::Wpa3Personal => "WPA3",
            WifiAuthType::Unknown => {
                if auth_type.akm.iter().any(|akm| akm.starts_with("802.1X")) {
                    "802.1X"
                } else if auth_type.akm.iter().any(|akm| akm == "OWE") {
                    "OWE"
                } else if auth_type.akm.is_empty() {
                    "WPA1"
                } else {
                    "UNKNOWN"
                }
            }
            _ => "UNKNOWN",
        };
        if !labels.contains(&label) {
            labels.push(label);
        }
    }
    if labels.is_empty() {
        "--".to_string()
    } else {
        labels.join(" ")
    }
}

// No idea why `libc::getpass()` or `nix::getpass()` does not exists, we have to
// it manually here.
fn getpass() -> Result<String, CliError> {
    let fd = stdin();
    let mut password = String::new();
    if fd.is_terminal() {
        let mut term = tcgetattr(&fd).map_err(|errno| {
            CliError::from(format!(
                "Failed to get terminal info from STDIN: {errno}"
            ))
        })?;
        let term_bak = term.clone();
        // Hide input
        term.local_flags.remove(LocalFlags::ECHO);
        // Show newline(user press enter)
        term.local_flags.insert(LocalFlags::ECHONL);

        tcsetattr(&fd, SetArg::TCSANOW, &term).map_err(|errno| {
            CliError::from(format!(
                "Failed to set STDIN terminal info for hiding password: \
                 {errno}"
            ))
        })?;

        print!("Please input password: ");
        stdout().flush().ok();
        let result = fd.read_line(&mut password);
        result?;
        // Restore the STDIN
        if let Err(errno) = tcsetattr(&fd, SetArg::TCSANOW, &term_bak) {
            log::warn!("Failed to restore STDIN terminal info: {errno}");
        };
    } else {
        fd.read_line(&mut password)?;
    }

    // Remove the tailing new line
    Ok(password.trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use nipart::WifiAuthTypeDetailed;

    use super::*;

    fn new_scan_result(
        ssid: &str,
        bssid: &str,
        frequency_mhz: u32,
        signal_percent: u8,
        auth_type: WifiAuthType,
    ) -> WifiScanResult {
        WifiScanResult::new(
            ssid.to_string(),
            Some("wlan0".to_string()),
            Some(bssid.to_string()),
            Some(frequency_mhz),
            Some(-50),
            Some(signal_percent),
            None,
            vec![WifiAuthTypeDetailed::new(auth_type, Vec::new(), Vec::new())],
        )
    }

    #[test]
    fn test_wifi_scan_table_has_nmcli_style_columns() {
        let wifi_cfgs = vec![
            new_scan_result(
                "Home",
                "02:00:00:00:00:03",
                2437,
                78,
                WifiAuthType::Wpa2Personal,
            ),
            new_scan_result(
                "Office",
                "02:00:00:00:00:0c",
                5180,
                42,
                WifiAuthType::Wpa3Personal,
            ),
        ];
        let active_ssids = HashSet::from(["Home".to_string()]);

        let table = wifi_scan_table(&wifi_cfgs, &active_ssids);
        let lines = table.lines().collect::<Vec<_>>();

        assert_eq!(
            lines[0],
            "IN-USE  BSSID              SSID              CHAN  BAND     \
             SIGNAL  BARS  SECURITY"
        );
        assert_eq!(
            lines[1],
            "*       02:00:00:00:00:03  Home              6     2.4 GHz  \
             78      ▂▄▆_  WPA2"
        );
        assert_eq!(
            lines[2],
            "        02:00:00:00:00:0C  Office            36    5 GHz    42      \
             ▂▄__  WPA3"
        );
    }

    #[test]
    fn test_wifi_scan_table_aligns_cjk_ssid() {
        let wifi_cfgs = vec![new_scan_result(
            "网网网网网",
            "02:00:00:00:00:03",
            2437,
            78,
            WifiAuthType::Wpa2Personal,
        )];

        let table = wifi_scan_table(&wifi_cfgs, &HashSet::new());

        assert_eq!(
            table.lines().nth(1).unwrap(),
            "        02:00:00:00:00:03  网网网网网        6     2.4 GHz  \
             78      ▂▄▆_  WPA2"
        );
    }

    #[test]
    fn test_wifi_scan_table_marks_whitespace_ssid_as_hidden() {
        let wifi_cfgs = vec![
            new_scan_result(
                "          ",
                "02:00:00:00:00:12",
                2462,
                42,
                WifiAuthType::Open,
            ),
            new_scan_result(
                "",
                "02:00:00:00:00:13",
                2462,
                42,
                WifiAuthType::Open,
            ),
            new_scan_result(
                "\0\0\0\0",
                "02:00:00:00:00:14",
                2462,
                42,
                WifiAuthType::Open,
            ),
        ];

        let table = wifi_scan_table(&wifi_cfgs, &HashSet::new());
        let lines = table.lines().collect::<Vec<_>>();

        assert!(lines[1].contains("02:00:00:00:00:12  --"));
        assert!(lines[2].contains("02:00:00:00:00:13  --"));
        assert!(lines[3].contains("02:00:00:00:00:14  --"));
        // Both rows must place the channel column at the same position.
        assert_eq!(
            lines[1].find("11"),
            lines[2].find("11"),
            "channel column misaligned:\n{lines:?}"
        );
        assert_eq!(
            lines[2].find("11"),
            lines[3].find("11"),
            "channel column misaligned:\n{lines:?}"
        );
    }

    #[test]
    fn test_wifi_scan_table_expands_ssid_column_for_long_ssid() {
        let wifi_cfgs = vec![
            new_scan_result(
                &"A".repeat(32),
                "02:00:00:00:00:0c",
                2462,
                42,
                WifiAuthType::Open,
            ),
            new_scan_result(
                "short",
                "02:00:00:00:00:0d",
                2462,
                42,
                WifiAuthType::Open,
            ),
        ];

        let table = wifi_scan_table(&wifi_cfgs, &HashSet::new());
        let lines = table.lines().collect::<Vec<_>>();

        assert!(lines[1].contains(&"A".repeat(32)));
        assert_eq!(
            lines[1].find("11"),
            lines[2].find("11"),
            "channel column misaligned:\n{lines:?}"
        );
    }

    #[test]
    fn test_display_ssid_replaces_control_characters() {
        assert_eq!(display_ssid("foo\0bar"), "foo?bar");
        assert_eq!(display_ssid("\0\0"), "--");
        assert_eq!(display_ssid("   "), "--");
        assert_eq!(display_ssid("Home Network"), "Home Network");
    }

    #[test]
    fn test_wifi_signal_color_matches_nmcli() {
        assert_eq!(wifi_signal_color(Some(90)), COLOR_GREEN);
        assert_eq!(wifi_signal_color(Some(78)), COLOR_YELLOW);
        assert_eq!(wifi_signal_color(Some(42)), COLOR_MAGENTA);
        assert_eq!(wifi_signal_color(Some(20)), COLOR_CYAN);
        assert_eq!(wifi_signal_color(Some(0)), COLOR_DIM);
        assert_eq!(wifi_signal_color(None), COLOR_DIM);
    }

    #[test]
    fn test_colorize_wifi_scan_table() {
        let wifi_cfgs = vec![new_scan_result(
            "Home",
            "02:00:00:00:00:03",
            2437,
            78,
            WifiAuthType::Wpa2Personal,
        )];
        let table = "header\nrow\n";

        assert_eq!(colorize_wifi_scan_table(table, &wifi_cfgs, false), table);
        assert_eq!(
            colorize_wifi_scan_table(table, &wifi_cfgs, true),
            "header\n\x1b[33mrow\x1b[0m\n"
        );
    }

    #[test]
    fn test_freq_to_channel() {
        assert_eq!(freq_to_channel(2412), Some(1));
        assert_eq!(freq_to_channel(2437), Some(6));
        assert_eq!(freq_to_channel(2484), Some(14));
        assert_eq!(freq_to_channel(5180), Some(36));
        assert_eq!(freq_to_channel(5825), Some(165));
        assert_eq!(freq_to_channel(5955), Some(1));
        assert_eq!(freq_to_channel(0), None);
    }

    #[test]
    fn test_freq_to_band() {
        assert_eq!(freq_to_band(2412), Some("2.4 GHz"));
        assert_eq!(freq_to_band(5180), Some("5 GHz"));
        assert_eq!(freq_to_band(5955), Some("6 GHz"));
        assert_eq!(freq_to_band(0), None);
    }

    #[test]
    fn test_security_string() {
        assert_eq!(security_string(&[]), "--");
        assert_eq!(
            security_string(&[WifiAuthTypeDetailed::new(
                WifiAuthType::Wpa2Personal,
                Vec::new(),
                Vec::new(),
            )]),
            "WPA2"
        );
        assert_eq!(
            security_string(&[
                WifiAuthTypeDetailed::new(
                    WifiAuthType::Wpa2Personal,
                    Vec::new(),
                    Vec::new(),
                ),
                WifiAuthTypeDetailed::new(
                    WifiAuthType::Wpa3Personal,
                    Vec::new(),
                    Vec::new(),
                ),
            ]),
            "WPA2 WPA3"
        );
    }
}
