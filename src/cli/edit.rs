// SPDX-License-Identifier: Apache-2.0

use std::{io::Write, os::unix::fs::OpenOptionsExt};

use nipart::{NetworkState, NipartClient, NipartQueryOption};

use crate::CliError;

pub(crate) struct CommandEdit;

impl CommandEdit {
    pub(crate) const CMD: &str = "edit";

    pub(crate) fn new_cmd() -> clap::Command {
        clap::Command::new("edit").about("Edit saved network state and apply")
    }

    pub(crate) async fn handle() -> Result<(), CliError> {
        let mut cli = NipartClient::new().await?;

        let saved_state: NetworkState =
            cli.query_network_state(NipartQueryOption::saved()).await?;

        let yaml_content = if saved_state.is_empty() {
            String::new()
        } else {
            rmsd_yaml::to_string(&saved_state)?
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

        let diff_net_state = match cli
            .apply_network_state(desired_state, Default::default())
            .await
        {
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
