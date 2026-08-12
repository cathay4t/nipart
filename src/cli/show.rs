// SPDX-License-Identifier: Apache-2.0

use nipart::{
    NetworkState, NipartClient, NipartInterface, NipartNoDaemon,
    NipartQueryOption, RouteEntry,
};

use crate::CliError;

pub(crate) struct CommandShow;

impl CommandShow {
    pub(crate) const CMD: &str = "show";

    pub(crate) fn new_cmd() -> clap::Command {
        clap::Command::new("show")
            .alias("s")
            .about("Query network state")
            .arg(
                clap::Arg::new("IFNAME")
                    .index(1)
                    .help("Show specific interface only"),
            )
            .arg(
                clap::Arg::new("NO_DAEMON")
                    .long("no-daemon")
                    .visible_alias("kernel")
                    .short('n')
                    .visible_short_alias('k')
                    .action(clap::ArgAction::SetTrue)
                    .help("Do not connect to nipart daemon"),
            )
            .arg(
                clap::Arg::new("SAVED")
                    .long("saved")
                    .short('s')
                    .action(clap::ArgAction::SetTrue)
                    .help("Show the daemon saved state only"),
            )
            .arg(
                clap::Arg::new("SHOW_SECRETS")
                    .long("show-secrets")
                    .action(clap::ArgAction::SetTrue)
                    .help("Show secrets(hide by default)"),
            )
    }

    pub(crate) async fn handle(
        matches: &clap::ArgMatches,
    ) -> Result<(), CliError> {
        let net_state = if matches.get_flag("NO_DAEMON") {
            if matches.get_flag("SAVED") {
                return Err("--no-daemon or --kernel cannot be used with \
                            --saved argument"
                    .into());
            }
            NipartNoDaemon::query_network_state(Default::default()).await?
        } else {
            let mut cli = NipartClient::new().await?;
            let mut opt = if matches.get_flag("SAVED") {
                NipartQueryOption::saved()
            } else {
                NipartQueryOption::running()
            };
            if matches.get_flag("SHOW_SECRETS") {
                opt = opt.include_secrets(true);
            }
            cli.query_network_state(opt).await?
        };
        let mut net_state =
            if let Some(ifname) = matches.get_one::<String>("IFNAME") {
                filter_net_state(&net_state, ifname)
            } else {
                net_state
            };

        if !matches.get_flag("SHOW_SECRETS") {
            net_state.hide_secrets();
        }

        println!("{}", rmsd_yaml::to_string(&net_state)?);

        Ok(())
    }
}

fn filter_net_state(
    net_state: &NetworkState,
    iface_name: &str,
) -> NetworkState {
    let mut ret = NetworkState::new();
    for iface in net_state.ifaces.iter() {
        if iface.kernel_iface_name() == iface_name {
            ret.ifaces.push(iface.clone())
        }
    }
    ret.routes.running =
        filter_routes(net_state.routes.running.as_deref(), iface_name);
    ret.routes.config =
        filter_routes(net_state.routes.config.as_deref(), iface_name);
    ret
}

/// Keep only route entries whose next hop interface matches `iface_name`.
/// Returns `None` when no route remains so the YAML output stays clean.
fn filter_routes(
    routes: Option<&[RouteEntry]>,
    iface_name: &str,
) -> Option<Vec<RouteEntry>> {
    let filtered: Vec<RouteEntry> = routes
        .map(|rts| {
            rts.iter()
                .filter(|rt| rt.next_hop_iface.as_deref() == Some(iface_name))
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

#[cfg(test)]
mod tests {
    use nipart::{
        BaseInterface, EthernetInterface, Interface, InterfaceType, Interfaces,
    };

    use super::*;

    fn new_route(destination: &str, next_hop_iface: &str) -> RouteEntry {
        let mut rt = RouteEntry::default();
        rt.destination = Some(destination.to_string());
        rt.next_hop_iface = Some(next_hop_iface.to_string());
        rt
    }

    fn new_iface(name: &str) -> Interface {
        Interface::Ethernet(Box::new(EthernetInterface::new(
            BaseInterface::new(name.to_string(), InterfaceType::Ethernet),
            None,
        )))
    }

    fn net_state_with_two_ifaces_and_routes() -> NetworkState {
        let mut net_state = NetworkState::new();
        net_state.ifaces =
            Interfaces::new(vec![new_iface("cunet"), new_iface("eth1")]);
        net_state.routes.running = Some(vec![
            new_route("0.0.0.0/0", "cunet"),
            new_route("192.0.2.0/24", "eth1"),
            new_route("198.51.100.0/24", "cunet"),
        ]);
        net_state.routes.config = Some(vec![
            new_route("10.0.0.0/8", "cunet"),
            new_route("172.16.0.0/12", "eth1"),
        ]);
        net_state
    }

    #[test]
    fn test_filter_net_state_keeps_iface_and_its_routes() {
        let net_state = net_state_with_two_ifaces_and_routes();
        let filtered = filter_net_state(&net_state, "cunet");

        assert_eq!(filtered.ifaces.iter().count(), 1);
        assert_eq!(
            filtered.ifaces.iter().next().unwrap().kernel_iface_name(),
            "cunet"
        );

        let running = filtered.routes.running.unwrap();
        assert_eq!(running.len(), 2);
        assert!(
            running
                .iter()
                .all(|rt| rt.next_hop_iface.as_deref() == Some("cunet"))
        );

        let config = filtered.routes.config.unwrap();
        assert_eq!(config.len(), 1);
        assert_eq!(config[0].destination.as_deref(), Some("10.0.0.0/8"));
    }

    #[test]
    fn test_filter_net_state_no_routes_yields_none() {
        let mut net_state = NetworkState::new();
        net_state.ifaces = Interfaces::new(vec![new_iface("cunet")]);
        net_state.routes.config = Some(vec![new_route("0.0.0.0/0", "eth1")]);

        let filtered = filter_net_state(&net_state, "cunet");

        assert_eq!(filtered.ifaces.iter().count(), 1);
        assert!(filtered.routes.running.is_none());
        assert!(filtered.routes.config.is_none());
    }

    #[test]
    fn test_filter_routes_empty_input() {
        assert!(filter_routes(None, "cunet").is_none());
        assert!(filter_routes(Some(&[]), "cunet").is_none());
    }
}
