// SPDX-License-Identifier: Apache-2.0

use std::{fs::Permissions, os::unix::fs::PermissionsExt};

use futures_channel::mpsc::{UnboundedReceiver, unbounded};
use futures_util::stream::StreamExt;
use nipart::{
    ErrorKind, InterfaceLinkEvent, NipartClient, NipartError,
    NipartIpcConnection, NipartIpcListener,
};
use tokio::sync::SetOnce;

use super::{api::process_api_connection, commander::NipartCommander};

pub(crate) static DAEMON_IS_ONLINE: SetOnce<()> = SetOnce::const_new();
const DAEMON_PID_FILE: &str = "/var/run/nipart/nipartd.pid";

#[derive(Debug, Clone)]
pub(crate) enum NipartManagerCmd {
    LinkEvent(Box<InterfaceLinkEvent>),
}

#[derive(Debug)]
pub(crate) struct NipartDaemon {
    api_ipc: NipartIpcListener,
    // For command send from managers of daemon.
    managers_ipc: UnboundedReceiver<NipartManagerCmd>,
    // Daemon will fork(tokio is controlling maximum threads) new thread for
    // each client connection, this commander will be cloned and move to all
    // forked threads.
    commander: NipartCommander,
    pid_file: String,
}

impl Drop for NipartDaemon {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.pid_file);
    }
}

impl NipartDaemon {
    pub(crate) async fn new() -> Result<Self, NipartError> {
        let api_ipc =
            NipartIpcListener::new(NipartClient::DEFAULT_SOCKET_PATH)?;
        // Make the API IPC globally read and writable for non-root user to
        // query and ping
        std::fs::set_permissions(
            NipartClient::DEFAULT_SOCKET_PATH,
            Permissions::from_mode(0o0666),
        )
        .map_err(|e| {
            NipartError::new(
                ErrorKind::Bug,
                format!(
                    "Failed to set permission of {} to 0666: {e}",
                    NipartClient::DEFAULT_SOCKET_PATH
                ),
            )
        })?;

        let (sender, receiver) = unbounded::<NipartManagerCmd>();

        let commander = NipartCommander::new(sender).await?;
        // Start a thread to load saved state instead of hanging
        let mut new_commander = commander.clone();
        tokio::spawn(async move {
            if let Err(e) = new_commander.load_saved_state().await {
                log::error!(
                    "Failed to load saved state: {e}, starting with empty \
                     state"
                );
            }
        });

        std::fs::create_dir_all(
            std::path::Path::new(DAEMON_PID_FILE)
                .parent()
                .unwrap_or_else(|| std::path::Path::new("/var/run")),
        )
        .map_err(|e| {
            NipartError::new(
                ErrorKind::Bug,
                format!("Failed to create pid file dir: {e}"),
            )
        })?;
        std::fs::write(DAEMON_PID_FILE, format!("{}\n", std::process::id()))
            .map_err(|e| {
                NipartError::new(
                    ErrorKind::Bug,
                    format!("Failed to write pid file {DAEMON_PID_FILE}: {e}"),
                )
            })?;

        Ok(Self {
            api_ipc,
            commander,
            managers_ipc: receiver,
            pid_file: DAEMON_PID_FILE.to_string(),
        })
    }

    /// Please run this function in a thread
    pub(crate) async fn run(&mut self) {
        let mut sigterm = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::terminate(),
        )
        .ok();
        let mut sigint = tokio::signal::unix::signal(
            tokio::signal::unix::SignalKind::interrupt(),
        )
        .ok();
        loop {
            tokio::select! {
                result = self.api_ipc.accept() => {
                    self.handle_api_connection(result).await;
                },
                cmd = self.managers_ipc.next() => {
                    if let Some(cmd) = cmd {
                        self.handle_manager_cmd(cmd).await;
                    }
                },
                _ = async {
                    if let Some(ref mut sig) = sigterm {
                        sig.recv().await;
                    } else {
                        std::future::pending().await
                    }
                } => {
                    log::info!("Received SIGTERM, shutting down");
                    break;
                },
                _ = async {
                    if let Some(ref mut sig) = sigint {
                        sig.recv().await;
                    } else {
                        std::future::pending().await
                    }
                } => {
                    log::info!("Received SIGINT, shutting down");
                    break;
                },
                else => break,
            }
        }
    }

    async fn handle_api_connection(
        &mut self,
        result: Result<NipartIpcConnection, NipartError>,
    ) {
        match result {
            Ok(conn) => {
                let commander = self.commander.clone();
                tokio::spawn(async move {
                    process_api_connection(conn, commander).await
                });
            }
            Err(e) => {
                log::info!("Ignoring failure of accepting API connection: {e}");
            }
        }
    }

    async fn handle_manager_cmd(&mut self, cmd: NipartManagerCmd) {
        // Since event worker is single thread, the event will be processed
        // as the order of its arrival.
        log::trace!("Got command from manager {cmd:?}");
        match cmd {
            NipartManagerCmd::LinkEvent(event) => {
                if let Err(e) =
                    self.commander.event_manager.handle_event(*event).await
                {
                    log::error!("{e}");
                }
            }
        }
    }
}
