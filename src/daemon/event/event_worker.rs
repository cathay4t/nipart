// SPDX-License-Identifier: Apache-2.0

use futures_channel::{mpsc::UnboundedReceiver, oneshot::Sender};
use nipart::{
    BaseInterface, ErrorKind, Interface, InterfaceAutoConnect, InterfaceIpv4,
    InterfaceIpv6, InterfaceLinkEvent, InterfaceState, InterfaceType,
    MergedNetworkState, NetworkState, NipartApplyOption, NipartError,
    NipartInterface, NipartNoDaemon, NipartQueryOption, RouteEntry, RouteState,
};

use super::super::{commander::NipartCommander, task::TaskWorker};

#[derive(Debug, Clone)]
pub(crate) enum NipartEventCmd {
    SetCommander(Box<NipartCommander>),
    HandleEvent(Box<InterfaceLinkEvent>),
}

impl std::fmt::Display for NipartEventCmd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SetCommander(_) => {
                write!(f, "set-commander")
            }
            Self::HandleEvent(event) => {
                write!(f, "handle-event:{event}")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NipartEventReply {
    None,
}

type FromManager = (
    NipartEventCmd,
    Sender<Result<NipartEventReply, NipartError>>,
);

#[derive(Debug)]
pub(crate) struct NipartEventWorker {
    receiver: UnboundedReceiver<FromManager>,
    commander: Option<NipartCommander>,
}

impl TaskWorker for NipartEventWorker {
    type Cmd = NipartEventCmd;
    type Reply = NipartEventReply;

    async fn new(
        receiver: UnboundedReceiver<FromManager>,
    ) -> Result<Self, NipartError> {
        Ok(Self {
            receiver,
            commander: None,
        })
    }

    fn receiver(&mut self) -> &mut UnboundedReceiver<FromManager> {
        &mut self.receiver
    }

    async fn process_cmd(
        &mut self,
        cmd: NipartEventCmd,
    ) -> Result<NipartEventReply, NipartError> {
        log::debug!("Processing event command: {cmd}");
        match cmd {
            NipartEventCmd::SetCommander(commander) => {
                self.commander = Some(*commander);
            }
            NipartEventCmd::HandleEvent(event) => {
                if let Err(e) = self.handle_event(*event).await {
                    log::error!("{e}");
                }
            }
        }
        Ok(NipartEventReply::None)
    }
}

impl NipartEventWorker {
    async fn handle_event(
        &mut self,
        mut event: InterfaceLinkEvent,
    ) -> Result<(), NipartError> {
        let Some(commander) = self.commander.as_mut() else {
            return Err(NipartError::new(
                ErrorKind::Bug,
                "NipartEventWorker::handle_event() invoked without commander \
                 set"
                .to_string(),
            ));
        };
        log::trace!("Handle link event {event}");
        let saved_state = commander.conf_manager.query_state().await?;
        let cur_state =
            NipartNoDaemon::query_network_state(NipartQueryOption::running())
                .await?;

        // Kernel event is always for kernel interface
        let cur_iface = cur_state.ifaces.kernel_ifaces.get(&event.iface_name);
        if let Some(cur_iface) = cur_iface {
            log::trace!("Current interface state: {cur_iface}");

            if event.ssid.is_none()
                && event.iface_type == InterfaceType::WifiPhy
                && let Interface::WifiPhy(cur_wifi_iface) = cur_iface
            {
                event.ssid = cur_wifi_iface.ssid().map(|s| s.to_string());
            }
        }

        let mut desired_state = NetworkState::default();

        // Purge IP if WIFI PHY interface is down or removed
        if !event.is_up && event.iface_type == InterfaceType::WifiPhy {
            let mut desired_iface = BaseInterface::new(
                event.iface_name.to_string(),
                event.iface_type.clone(),
            );
            desired_iface.state = if cur_iface.is_some() {
                InterfaceState::Up
            } else {
                // WIFI PHY interface removed.
                InterfaceState::Absent
            };
            // Purge IP
            desired_iface.ipv4 = Some(InterfaceIpv4::new_disabled());
            desired_iface.ipv6 = Some(InterfaceIpv6::new_disabled());
            log::trace!(
                "{}: link down on wifi-phy, purging IP stack: {desired_iface}",
                event.iface_name
            );
            desired_state.ifaces.push(desired_iface.into());
        }

        for saved_iface in saved_state.ifaces.iter() {
            if event.iface_type == InterfaceType::WifiPhy {
                if let Some(new_iface) =
                    handle_wifi_phy_event(&event, saved_iface)
                {
                    log::trace!("Pending apply config: {new_iface}");
                    desired_state.ifaces.push(new_iface);
                }
            } else if cur_iface
                .map(|cur_iface| saved_iface.is_match(cur_iface))
                .unwrap_or(false)
                && saved_iface.base_iface().auto_connect.is_none()
                && !event.is_delete
            {
                log::trace!("Pending apply config for {saved_iface}");
                desired_state.ifaces.push(saved_iface.clone());
            }

            if let Some((new_iface, routes)) = handle_event_auto_connect(
                &event,
                saved_iface,
                &saved_state,
                &cur_state,
            ) {
                desired_state.ifaces.push(new_iface);
                let config_routes =
                    desired_state.routes.config.get_or_insert_default();
                for route in routes {
                    log::trace!("Pending apply route {route}");
                    config_routes.push(route);
                }
            }
        }

        if !desired_state.is_empty() {
            log::trace!("Applying desired state {desired_state}");
            let merged_state = MergedNetworkState::new(
                desired_state,
                cur_state,
                None,
                NipartApplyOption::new().no_verify(),
            )?;
            commander.apply_merged_state(None, &merged_state).await?;
        } else {
            log::trace!("No change required for event {event}");
        }

        Ok(())
    }
}

fn is_route_matching_iface(rt: &RouteEntry, iface: &Interface) -> bool {
    match rt.next_hop_iface.as_deref() {
        Some(name) if name == iface.kernel_iface_name() => true,
        Some(name)
            if Some(name) == iface.base_iface().profile_name.as_deref() =>
        {
            true
        }
        Some(name) if name == iface.name() => true,
        _ => false,
    }
}

fn gen_desired_iface_up(
    saved_iface: &Interface,
    saved_state: &NetworkState,
) -> (Interface, Vec<RouteEntry>) {
    let mut ret_routes: Vec<RouteEntry> = Vec::new();
    let mut new_iface = saved_iface.clone();
    new_iface.base_iface_mut().state = InterfaceState::Up;
    new_iface.base_iface_mut().auto_connect = None;

    // Include routes to this interface also
    if !new_iface.is_userspace()
        && let Some(config_rts) = saved_state.routes.config.as_ref()
    {
        for rt in config_rts
            .iter()
            .filter(|rt| is_route_matching_iface(rt, saved_iface))
        {
            ret_routes.push(rt.clone());
        }
    }

    (new_iface, ret_routes)
}

fn gen_desired_iface_down(
    auto_connect: &InterfaceAutoConnect,
    saved_iface: &Interface,
    saved_state: &NetworkState,
) -> (Interface, Vec<RouteEntry>) {
    let mut new_iface = saved_iface.clone();
    let mut ret_routes: Vec<RouteEntry> = Vec::new();
    // We cannot bring interface down when `auto-connect` is `true`,
    // otherwise that interface will never up again.
    if auto_connect != &InterfaceAutoConnect::AutoConnect
        && saved_iface.iface_type() != &InterfaceType::WifiCfg
    {
        new_iface.base_iface_mut().state = if saved_iface.is_virtual() {
            InterfaceState::Absent
        } else {
            InterfaceState::Down
        };
    }
    new_iface.base_iface_mut().auto_connect = None;
    new_iface.base_iface_mut().ipv4 = Some(InterfaceIpv4::new_disabled());
    new_iface.base_iface_mut().ipv6 = Some(InterfaceIpv6::new_disabled());

    // Remove routes to this interface also
    if !new_iface.is_userspace()
        && let Some(config_rts) = saved_state.routes.config.as_ref()
    {
        for rt in config_rts
            .iter()
            .filter(|rt| is_route_matching_iface(rt, saved_iface))
        {
            let mut new_route = rt.clone();
            new_route.state = Some(RouteState::Absent);
            ret_routes.push(new_route);
        }
    }

    (new_iface, ret_routes)
}

fn wifi_cfg_to_wifi_phy(
    iface_name: &str,
    saved_iface: &Interface,
) -> Interface {
    let mut desired = saved_iface.base_iface().clone();
    desired.name = iface_name.to_string();
    desired.kernel_iface_name = iface_name.to_string();
    desired.iface_type = InterfaceType::WifiPhy;

    desired.into()
}

fn handle_event_auto_connect(
    event: &InterfaceLinkEvent,
    saved_iface: &Interface,
    saved_state: &NetworkState,
    cur_state: &NetworkState,
) -> Option<(Interface, Vec<RouteEntry>)> {
    let auto_connect = saved_iface.base_iface().auto_connect.as_ref()?;

    match saved_iface.process_auto_connect(event, &cur_state.ifaces) {
        None => {
            log::trace!("No auto-connect action for {event}");
            None
        }
        Some(false) => {
            let (new_iface, routes) =
                gen_desired_iface_down(auto_connect, saved_iface, saved_state);
            log::trace!(
                "Pending apply action to bring {} down",
                event.iface_name
            );
            if !routes.is_empty() {
                log::trace!("Pending route changes: {routes:?}");
            }
            Some((new_iface, routes))
        }
        Some(true) => {
            let (new_iface, routes) =
                gen_desired_iface_up(saved_iface, saved_state);
            log::trace!(
                "Pending apply action to bring {} up",
                event.iface_name
            );
            if !routes.is_empty() {
                log::trace!("Pending route changes: {routes:?}");
            }
            Some((new_iface, routes))
        }
    }
}

fn handle_wifi_phy_event(
    event: &InterfaceLinkEvent,
    saved_iface: &Interface,
) -> Option<Interface> {
    if !event.is_up && saved_iface.iface_type() == &InterfaceType::WifiPhy {
        // Already processed above to purge IP on this wifi-phy interface.
        None
    } else if !event.is_up
        && !event.is_delete
        && event.iface_type == InterfaceType::WifiPhy
        && let Interface::WifiCfg(saved_wifi_cfg) = saved_iface
        && (saved_wifi_cfg.parent().is_none()
            || saved_wifi_cfg.parent() == Some(event.iface_name.as_str()))
    {
        // When new WIFI PHY found, we should setup `bind-to-any` WIFI to
        // it.
        let ssid = saved_wifi_cfg.ssid()?;
        let mut desired_iface = saved_iface.clone();
        // WifiCfg bind to any SSID should changed to event
        // interface only, so other interface is not impacted
        if let Interface::WifiCfg(iface) = &mut desired_iface
            && let Some(wifi_cfg) = iface.wifi.as_mut()
        {
            wifi_cfg.base_iface = Some(event.iface_name.to_string());
        } else {
            unreachable!();
        }
        log::trace!(
            "Pending apply wifi-cfg {ssid} on wifi-phy: {}",
            event.iface_name,
        );
        Some(desired_iface)
    } else if event.is_up
        && event.ssid.is_some()
        && let Interface::WifiCfg(saved_wifi_iface) = saved_iface
    {
        if event.ssid.as_deref() == saved_wifi_iface.ssid() {
            let new_iface =
                wifi_cfg_to_wifi_phy(event.iface_name.as_str(), saved_iface);
            log::debug!("Pending apply wifi-cfg config: {new_iface}");
            Some(new_iface)
        } else {
            // Since the WIFI interface is already up, we should not
            // try to configure more SSID on it which should be done
            // at link_down event. Hence we continue regardless whether
            // SSID match or not.
            None
        }
    } else {
        None
    }
}
