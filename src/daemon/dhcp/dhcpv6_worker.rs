// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use futures_channel::{
    mpsc::{UnboundedReceiver, UnboundedSender, unbounded},
    oneshot::Sender,
};
use futures_util::StreamExt;
use mozim::{DhcpV6Client, DhcpV6Config, DhcpV6Mode, DhcpV6State};
use nipart::{
    BaseInterface, DhcpState, ErrorKind, NipartError, NipartNoDaemon,
};

use crate::TaskWorker;

const DHCPV6_INIT_RETRY_COUNT: usize = 30;
const DHCPV6_INIT_RETRY_INTERVAL_MS: u64 = 1000;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NipartDhcpV6Cmd {
    StartIfaceDhcp(Box<BaseInterface>),
    StopIfaceDhcp(String),
    Query,
}

impl std::fmt::Display for NipartDhcpV6Cmd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StartIfaceDhcp(base_iface) => {
                write!(f, "start-iface-dhcpv6:{}", base_iface.name)
            }
            Self::StopIfaceDhcp(iface) => {
                write!(f, "stop-iface-dhcpv6:{iface}")
            }
            Self::Query => {
                write!(f, "query-dhcpv6")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NipartDhcpV6Reply {
    None,
    QueryReply(HashMap<String, DhcpState>),
}

type FromManager = (
    NipartDhcpV6Cmd,
    Sender<Result<NipartDhcpV6Reply, NipartError>>,
);

#[derive(Debug)]
pub(crate) struct NipartDhcpV6Worker {
    threads: HashMap<String, NipartDhcpV6Thread>,
    receiver: UnboundedReceiver<FromManager>,
}

impl TaskWorker for NipartDhcpV6Worker {
    type Cmd = NipartDhcpV6Cmd;
    type Reply = NipartDhcpV6Reply;

    async fn new(
        receiver: UnboundedReceiver<(
            Self::Cmd,
            Sender<Result<Self::Reply, NipartError>>,
        )>,
    ) -> Result<Self, NipartError> {
        Ok(Self {
            threads: HashMap::new(),
            receiver,
        })
    }

    fn receiver(&mut self) -> &mut UnboundedReceiver<FromManager> {
        &mut self.receiver
    }

    async fn process_cmd(
        &mut self,
        cmd: NipartDhcpV6Cmd,
    ) -> Result<NipartDhcpV6Reply, NipartError> {
        match cmd {
            NipartDhcpV6Cmd::StartIfaceDhcp(base_iface) => {
                let iface_name = base_iface.name.clone();
                let thread = NipartDhcpV6Thread::new(*base_iface).await?;
                self.threads.insert(iface_name.clone(), thread);
                log::debug!("DHCPv6 thread started on interface {iface_name}");
                Ok(NipartDhcpV6Reply::None)
            }
            NipartDhcpV6Cmd::StopIfaceDhcp(iface) => {
                self.threads.remove(&iface);
                Ok(NipartDhcpV6Reply::None)
            }
            NipartDhcpV6Cmd::Query => {
                let mut ret = HashMap::new();
                for (iface_name, thread) in self.threads.iter() {
                    ret.insert(iface_name.to_string(), thread.get_state()?);
                }

                Ok(NipartDhcpV6Reply::QueryReply(ret))
            }
        }
    }
}

#[derive(Debug, Default)]
struct NipartDhcpV6ShareData {
    state: DhcpState,
}

#[derive(Debug)]
pub(crate) struct NipartDhcpV6Thread {
    pub(crate) base_iface: BaseInterface,
    // No need to send any data. Dropping this Sender will cause
    // Receiver.recv() got None which trigger DHCP thread quit.
    _quit_notifer: UnboundedSender<()>,
    share_data: Arc<Mutex<NipartDhcpV6ShareData>>,
}

impl NipartDhcpV6Thread {
    pub(crate) async fn new(
        base_iface: BaseInterface,
    ) -> Result<Self, NipartError> {
        let (sender, receiver) = unbounded();
        let ret = Self {
            base_iface: base_iface.clone(),
            _quit_notifer: sender,
            share_data: Arc::new(Mutex::new(NipartDhcpV6ShareData::default())),
        };

        let share_data = ret.share_data.clone();
        tokio::spawn(async move {
            if let Err(e) = dhcp_thread(base_iface, receiver, share_data).await
            {
                log::error!("{e}");
            }
        });
        Ok(ret)
    }

    pub(crate) fn get_state(&self) -> Result<DhcpState, NipartError> {
        match self.share_data.lock() {
            Ok(data) => Ok(data.state.clone()),
            Err(e) => Err(NipartError::new(
                ErrorKind::Bug,
                format!(
                    "Failed to lock share data of DHCPv6 thread for interface \
                     {}: {e}",
                    self.base_iface.name
                ),
            )),
        }
    }
}

async fn dhcp_thread(
    base_iface: BaseInterface,
    mut quit_indicator: UnboundedReceiver<()>,
    share_data: Arc<Mutex<NipartDhcpV6ShareData>>,
) -> Result<(), NipartError> {
    log::debug!(
        "Waiting link carrier up for interface {}/{} before start DHCPv6",
        base_iface.name,
        base_iface.iface_type
    );
    NipartNoDaemon::wait_link_carrier_up(base_iface.name.as_str()).await?;
    log::debug!(
        "Interface {}/{} link carrier is up, starting DHCPv6 process",
        base_iface.name,
        base_iface.iface_type
    );
    // The DHCPv6 client needs the IPv6 link-local address as its source
    // address, enable IPv6 first if it was previously disabled.
    if let Err(e) = NipartNoDaemon::enable_ipv6(base_iface.name.as_str()).await
    {
        set_state(&share_data, DhcpState::Error(e.to_string()), &base_iface)?;
        return Err(e);
    }
    set_state(&share_data, DhcpState::Running, &base_iface)?;

    let mut dhcp_client =
        match init_dhcp_client(&base_iface, &mut quit_indicator).await {
            Ok(Some(client)) => client,
            Ok(None) => {
                // Quit requested during client init.
                return Ok(());
            }
            Err(e) => {
                set_state(
                    &share_data,
                    DhcpState::Error(e.to_string()),
                    &base_iface,
                )?;
                return Err(e);
            }
        };

    let result = loop {
        tokio::select! {
            result = dhcp_client.run() => {
                match result {
                    Ok(DhcpV6State::Done(lease)) => {
                        log::info!(
                            "DHCPv6 on {}({}) got lease {}",
                            base_iface.name,
                            base_iface.iface_type,
                            lease.address,
                        );
                        set_state(&share_data, DhcpState::Done, &base_iface)?;
                        if let Err(e) = NipartNoDaemon::apply_dhcpv6_lease(
                            &base_iface,
                            &lease
                        ).await {
                            break Err::<(), NipartError>(e);
                        }
                    }
                    Ok(dhcp_state) => {
                        log::info!(
                            "DHCPv6 on {}({}) reach {} state",
                            base_iface.name,
                            base_iface.iface_type,
                            dhcp_state
                        );
                    }
                    Err(e) => {
                        log::warn!(
                            "DHCPv6 on {}({}) failed, restarting client: {e}",
                            base_iface.name,
                            base_iface.iface_type,
                        );
                        set_state(
                            &share_data,
                            DhcpState::Running,
                            &base_iface,
                        )?;
                        // A single transient error must not kill the
                        // client permanently: right after wifi association
                        // the IPv6 link-local address may still be
                        // tentative, making the UDP socket bind fail even
                        // though `DhcpV6Client::init()` succeeded. Give
                        // the kernel a moment, then re-resolve the
                        // interface (fresh link-local address) and retry.
                        dhcp_client.clean_up();
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                            _ = quit_indicator.next() => {
                                log::info!(
                                    "Stopped DHCPv6 on {}({}) after error",
                                    base_iface.name,
                                    base_iface.iface_type,
                                );
                                return Ok(());
                            }
                        }
                        if let Err(wait_err) = NipartNoDaemon::wait_link_carrier_up(
                            base_iface.name.as_str(),
                        )
                        .await
                        {
                            set_state(
                                &share_data,
                                DhcpState::Error(wait_err.to_string()),
                                &base_iface,
                            )?;
                            break Err(wait_err);
                        }
                        match init_dhcp_client(
                            &base_iface,
                            &mut quit_indicator,
                        )
                        .await
                        {
                            Ok(Some(new_client)) => {
                                dhcp_client = new_client;
                            }
                            Ok(None) => {
                                // Quit requested during client init.
                                return Ok(());
                            }
                            Err(init_err) => {
                                set_state(
                                    &share_data,
                                    DhcpState::Error(init_err.to_string()),
                                    &base_iface,
                                )?;
                                break Err(init_err);
                            }
                        }
                    }
                }
            }
            _ = quit_indicator.next() => {
                log::info!(
                    "Stopped DHCPv6 on {}({})",
                    base_iface.name,
                    base_iface.iface_type,
                );
                return Ok(());
            }
        }
    };

    if let Err(e) = result {
        set_state(&share_data, DhcpState::Error(e.to_string()), &base_iface)?;
    }
    Ok(())
}

fn set_state(
    share_data: &Arc<Mutex<NipartDhcpV6ShareData>>,
    state: DhcpState,
    base_iface: &BaseInterface,
) -> Result<(), NipartError> {
    match share_data.lock() {
        Ok(mut share_data) => {
            share_data.state = state;
            Ok(())
        }
        Err(e) => Err(NipartError::new(
            ErrorKind::Bug,
            format!(
                "Failed to lock DHCPv6 {}({}) share data: {e}",
                base_iface.name, base_iface.iface_type,
            ),
        )),
    }
}

/// Initialize the DHCPv6 client.
/// The client init resolves the interface link-local address via netlink,
/// retry a few times to cover the window between link carrier up and the
/// link-local address being assigned on the interface.
async fn init_dhcp_client(
    base_iface: &BaseInterface,
    quit_indicator: &mut UnboundedReceiver<()>,
) -> Result<Option<DhcpV6Client>, NipartError> {
    // Whether we already forced the kernel to regenerate the IPv6 link-local
    // address for this round of init attempts.
    let mut link_local_regenerated = false;
    for retry_count in 0..DHCPV6_INIT_RETRY_COUNT {
        let mut dhcp_config = DhcpV6Config::new(
            base_iface.name.as_str(),
            DhcpV6Mode::NonTemporaryAddresses,
        );
        if let Some(iface_index) = base_iface.iface_index {
            dhcp_config.set_iface_index(iface_index);
        }
        match DhcpV6Client::init(dhcp_config, None).await {
            Ok(client) => return Ok(Some(client)),
            Err(e) => {
                // The init failure is usually caused by the interface holding
                // no IPv6 link-local address, flip `disable_ipv6` once so the
                // kernel regenerates the link-local address for it.
                if !link_local_regenerated {
                    link_local_regenerated = true;
                    log::info!(
                        "DHCPv6 client init failed on iface {}/{}, forcing \
                         the kernel to regenerate the IPv6 link-local \
                         address: {e}",
                        base_iface.name,
                        base_iface.iface_type,
                    );
                    if let Err(regen_err) =
                        NipartNoDaemon::regenerate_link_local(
                            base_iface.name.as_str(),
                        )
                        .await
                    {
                        log::warn!(
                            "Failed to regenerate IPv6 link-local address on \
                             iface {}/{}: {regen_err}",
                            base_iface.name,
                            base_iface.iface_type,
                        );
                    }
                }
                if retry_count + 1 == DHCPV6_INIT_RETRY_COUNT {
                    return Err(NipartError::new(
                        ErrorKind::Bug,
                        format!(
                            "Failed to start DHCPv6 client on iface {}/{}: {e}",
                            base_iface.name, base_iface.iface_type,
                        ),
                    ));
                }
                log::debug!(
                    "Failed to init DHCPv6 client on iface {}/{}, retry \
                     ({retry_count}/{DHCPV6_INIT_RETRY_COUNT}): {e}",
                    base_iface.name,
                    base_iface.iface_type,
                );
                tokio::select! {
                    _ = tokio::time::sleep(
                        std::time::Duration::from_millis(
                            DHCPV6_INIT_RETRY_INTERVAL_MS
                        )
                    ) => {}
                    _ = quit_indicator.next() => {
                        log::info!(
                            "Stopped DHCPv6 on {}({}) during client init",
                            base_iface.name,
                            base_iface.iface_type,
                        );
                        return Ok(None);
                    }
                }
            }
        }
    }
    unreachable!()
}
