// SPDX-License-Identifier: Apache-2.0

use std::{io::Write, os::unix::fs::OpenOptionsExt};

use nipart::{
    Interface, InterfaceType, NetworkState, NipartApplyOption, NipartClient,
    NipartInterface, NipartNoDaemon, NipartQueryOption, RouteEntry,
};
use nix::unistd::Uid;

use crate::CliError;

pub(crate) struct CommandEdit;

impl CommandEdit {
    pub(crate) const CMD: &str = "edit";

    pub(crate) fn new_cmd() -> clap::Command {
        clap::Command::new("edit")
            .about("Edit saved network state and apply")
            .arg(
                clap::Arg::new("IFNAME_OR_PROFILE")
                    .index(1)
                    .help("Interface name or saved profile name"),
            )
            .arg(
                clap::Arg::new("TAKE_CURRENT")
                    .long("take-current")
                    .action(clap::ArgAction::SetTrue)
                    .help(
                        "Start editing from the current running state instead \
                         of the saved state",
                    ),
            )
            .arg(
                clap::Arg::new("NO_DAEMON")
                    .long("no-daemon")
                    .short('n')
                    .action(clap::ArgAction::SetTrue)
                    .help("Do not connect to nipart daemon"),
            )
    }

    pub(crate) async fn handle(
        matches: &clap::ArgMatches,
    ) -> Result<(), CliError> {
        if !Uid::effective().is_root() {
            return Err("npt edit requires root permission".into());
        }

        let name = matches
            .get_one::<String>("IFNAME_OR_PROFILE")
            .map(String::as_str);
        let no_daemon = matches.get_flag("NO_DAEMON");
        let take_current = matches.get_flag("TAKE_CURRENT") || no_daemon;

        let mut cli = if no_daemon {
            None
        } else {
            Some(NipartClient::new().await?)
        };

        let net_state = if no_daemon {
            let cur_state =
                NipartNoDaemon::query_network_state(Default::default()).await?;
            filter_edit_state(&cur_state, name, "current")?
        } else {
            let cli = cli.as_mut().unwrap();
            let query_opt = if take_current {
                NipartQueryOption::running()
            } else {
                NipartQueryOption::saved()
            };
            let state = cli.query_network_state(query_opt).await?;
            if take_current {
                filter_edit_state(&state, name, "current")?
            } else if let Ok(edit_state) =
                filter_edit_state(&state, name, "saved")
            {
                edit_state
            } else if let Some(name) = name {
                // A `wifi-phy` may have no saved interface of its own: its
                // saved configuration is carried by the `wifi-cfg` profiles
                // bound to it.
                filter_edit_state_from_saved_wifi_cfgs(&state, name)?
            } else {
                return Err(
                    "No interface or profile found in saved state".into()
                );
            }
        };

        let yaml_content = if net_state.is_empty() {
            String::new()
        } else {
            rmsd_yaml::to_string(&net_state)?
        };

        let tmp_dir = std::env::temp_dir();
        let tmp_file_name = format!("npt-edit-{}.yml", uuid::Uuid::now_v7());
        let tmp_file_path = tmp_dir.join(&tmp_file_name);

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp_file_path)?;
        file.write_all(yaml_content.as_bytes())?;
        drop(file);

        let editor = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "vim".into());

        let status = std::process::Command::new(&editor)
            .arg(&tmp_file_path)
            .status()?;

        if !status.success() {
            std::fs::remove_file(&tmp_file_path).ok();
            return Err(format!("Editor '{editor}' exited with error").into());
        }

        let edited_content = std::fs::read_to_string(&tmp_file_path)?;

        if edited_content.trim().is_empty() {
            std::fs::remove_file(&tmp_file_path).ok();
            println!("Nothing changed");
            return Ok(());
        }

        let desired_state = match NetworkState::new_from_yaml(&edited_content) {
            Ok(state) => state,
            Err(e) => {
                eprintln!("Failed to parse YAML: {e}");
                eprintln!(
                    "Your edited configuration is preserved at: {}",
                    tmp_file_path.display()
                );
                return Err(e.into());
            }
        };

        let apply_result = if no_daemon {
            let mut opt = NipartApplyOption::default();
            opt.dhcp_in_no_daemon = true;
            NipartNoDaemon::apply_network_state(desired_state, opt).await
        } else {
            cli.as_mut()
                .unwrap()
                .apply_network_state(desired_state, Default::default())
                .await
        };
        let diff_net_state = match apply_result {
            Ok(state) => {
                std::fs::remove_file(&tmp_file_path).ok();
                state
            }
            Err(e) => {
                eprintln!("Failed to apply network state: {e}");
                eprintln!(
                    "Your edited configuration is preserved at: {}",
                    tmp_file_path.display()
                );
                return Err(e.into());
            }
        };

        if diff_net_state.is_empty() {
            println!("Nothing changed");
        } else {
            println!(
                "Changed state:\n---\n{}",
                rmsd_yaml::to_string(&diff_net_state)?
            );
        }

        Ok(())
    }
}

fn filter_edit_state(
    net_state: &NetworkState,
    name: Option<&str>,
    source: &str,
) -> Result<NetworkState, CliError> {
    let Some(name) = name else {
        return Ok(net_state.clone());
    };

    let mut ifaces: Vec<&Interface> = net_state
        .ifaces
        .iter()
        .filter(|iface| iface_matches_name(iface, name))
        .collect();
    if ifaces.is_empty() {
        return Err(format!(
            "No interface or profile '{name}' found in {source} state"
        )
        .into());
    }

    let wifi_phys: Vec<&Interface> = ifaces
        .iter()
        .copied()
        .filter(|iface| iface.iface_type() == &InterfaceType::WifiPhy)
        .collect();
    if !wifi_phys.is_empty() {
        for wifi_cfg in net_state
            .ifaces
            .iter()
            .filter(|iface| iface.iface_type() == &InterfaceType::WifiCfg)
        {
            if wifi_phys
                .iter()
                .any(|phy| wifi_cfg_applies_to_iface(wifi_cfg, phy))
                && !ifaces.contains(&wifi_cfg)
            {
                ifaces.push(wifi_cfg);
            }
        }
    }

    Ok(filter_state_with_ifaces(net_state, ifaces, &[]))
}

fn filter_edit_state_from_saved_wifi_cfgs(
    saved_state: &NetworkState,
    name: &str,
) -> Result<NetworkState, CliError> {
    let ifaces: Vec<&Interface> = saved_state
        .ifaces
        .iter()
        .filter(|iface| iface.iface_type() == &InterfaceType::WifiCfg)
        .filter(|wifi_cfg| wifi_cfg_has_base_iface(wifi_cfg, name))
        .collect();
    if ifaces.is_empty() {
        return Err(format!(
            "No interface or profile '{name}' found in saved state"
        )
        .into());
    }

    Ok(filter_state_with_ifaces(saved_state, ifaces, &[name]))
}

fn filter_state_with_ifaces(
    net_state: &NetworkState,
    ifaces: Vec<&Interface>,
    route_names: &[&str],
) -> NetworkState {
    let mut ret = NetworkState::new();
    for iface in &ifaces {
        ret.ifaces.push((*iface).clone());
    }
    ret.routes.running = filter_routes(
        net_state.routes.running.as_deref(),
        &ifaces,
        route_names,
    );
    ret.routes.config =
        filter_routes(net_state.routes.config.as_deref(), &ifaces, route_names);
    ret
}

fn iface_matches_name(iface: &Interface, name: &str) -> bool {
    iface.name() == name
        || iface.kernel_iface_name() == name
        || iface.base_iface().profile_name.as_deref() == Some(name)
}

/// Keep only routes whose next hop interface matches the selected interface
/// by kernel name, logical name or profile name.
fn filter_routes(
    routes: Option<&[RouteEntry]>,
    ifaces: &[&Interface],
    route_names: &[&str],
) -> Option<Vec<RouteEntry>> {
    let filtered: Vec<RouteEntry> = routes
        .map(|rts| {
            rts.iter()
                .filter(|rt| {
                    route_matches_name(rt, route_names)
                        || ifaces
                            .iter()
                            .any(|iface| route_matches_iface(rt, iface))
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if filtered.is_empty() {
        None
    } else {
        Some(filtered)
    }
}

fn route_matches_name(rt: &RouteEntry, route_names: &[&str]) -> bool {
    rt.next_hop_iface
        .as_deref()
        .is_some_and(|iface| route_names.contains(&iface))
}

fn wifi_cfg_applies_to_iface(
    wifi_cfg: &Interface,
    phy_iface: &Interface,
) -> bool {
    let Interface::WifiCfg(wifi_cfg) = wifi_cfg else {
        return false;
    };
    match wifi_cfg.parent() {
        None => true,
        Some(base_iface) => {
            base_iface == phy_iface.name()
                || base_iface == phy_iface.kernel_iface_name()
                || phy_iface.base_iface().profile_name.as_deref()
                    == Some(base_iface)
        }
    }
}

fn wifi_cfg_has_base_iface(wifi_cfg: &Interface, name: &str) -> bool {
    matches!(
        wifi_cfg,
        Interface::WifiCfg(wifi_cfg) if wifi_cfg.parent() == Some(name)
    )
}

fn route_matches_iface(rt: &RouteEntry, iface: &Interface) -> bool {
    let Some(next_hop_iface) = rt.next_hop_iface.as_deref() else {
        return false;
    };
    next_hop_iface == iface.kernel_iface_name()
        || next_hop_iface == iface.name()
        || iface.base_iface().profile_name.as_deref() == Some(next_hop_iface)
}

#[cfg(test)]
#[path = "unit_tests/edit.rs"]
mod tests;
