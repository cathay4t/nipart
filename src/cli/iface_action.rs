// SPDX-License-Identifier: Apache-2.0

use nipart::{NetworkState, NipartClient};

use crate::CliError;

pub(crate) struct CommandUp;

impl CommandUp {
    pub(crate) const CMD: &str = "up";

    pub(crate) fn new_cmd() -> clap::Command {
        clap::Command::new("up")
            .about("Bring an interface or saved profile up")
            .arg(
                clap::Arg::new("IFNAME_OR_PROFILE")
                    .required(true)
                    .index(1)
                    .help("Interface name or saved profile name"),
            )
    }

    pub(crate) async fn handle(
        matches: &clap::ArgMatches,
    ) -> Result<(), CliError> {
        let name = matches
            .get_one::<String>("IFNAME_OR_PROFILE")
            .ok_or("Missing interface or profile name")?;
        let mut cli = NipartClient::new().await?;
        let diff_state = cli.up_interface(name).await?;
        print_result(name, "up", diff_state)
    }
}

pub(crate) struct CommandDown;

impl CommandDown {
    pub(crate) const CMD: &str = "down";

    pub(crate) fn new_cmd() -> clap::Command {
        clap::Command::new("down")
            .about("Bring an interface or saved profile down")
            .arg(
                clap::Arg::new("IFNAME_OR_PROFILE")
                    .required(true)
                    .index(1)
                    .help("Interface name or saved profile name"),
            )
    }

    pub(crate) async fn handle(
        matches: &clap::ArgMatches,
    ) -> Result<(), CliError> {
        let name = matches
            .get_one::<String>("IFNAME_OR_PROFILE")
            .ok_or("Missing interface or profile name")?;
        let mut cli = NipartClient::new().await?;
        let diff_state = cli.down_interface(name).await?;
        print_result(name, "down", diff_state)
    }
}

fn print_result(
    name: &str,
    action: &str,
    mut diff_state: NetworkState,
) -> Result<(), CliError> {
    diff_state.hide_secrets();
    if diff_state.is_empty() {
        println!("Interface {name} is {action}");
    } else {
        println!(
            "Interface {name} is {action}:\n---\n{}",
            rmsd_yaml::to_string(&diff_state)?
        );
    }
    Ok(())
}
