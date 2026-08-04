// SPDX-License-Identifier: Apache-2.0

use futures_channel::{mpsc::UnboundedReceiver, oneshot::Sender};
use nipart::{
    BaseInterface, ErrorKind, Interface, InterfaceAutoConnect, InterfaceIpv4,
    InterfaceIpv6, InterfaceLinkEvent, InterfaceLinkState, InterfaceState,
    InterfaceType, MergedNetworkState, NetworkState, NipartApplyOption,
    NipartError, NipartInterface, NipartNoDaemon, NipartQueryOption,
    RouteEntry, RouteState,
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

        // Skip stale link-down events: when the interface's current link
        // state is already up, a queued down event is a leftover of an
        // earlier transient state (e.g. the device driver initialization
        // burst at boot, or the monitor emitting the link dump on resume).
        // Processing it would purge the IP and routes that the boot apply
        // has just configured, and the later up event does not reliably
        // restore them (the partial merge may drop routes of interfaces
        // that are temporarily IP-disabled).
        if is_stale_link_down_event(&event, cur_iface) {
            log::trace!(
                "Ignoring stale link-down event {event}: current link state \
                 is up"
            );
            return Ok(());
        }

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
            if event.iface_type == InterfaceType::WifiPhy
                && let Some(new_iface) =
                    handle_wifi_phy_event(&event, saved_iface)
            {
                log::trace!("Pending apply config: {new_iface}");
                desired_state.ifaces.push(new_iface);
            }

            // `auto-connect` defaults to `true` when not defined, hence
            // interfaces without `auto-connect` are handled here as well.
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

/// Whether a link-down event is stale, i.e. the interface's current kernel
/// link state is already up so the down event can only be a leftover of an
/// earlier transient state (e.g. the boot-time device initialization burst
/// or the monitor link dump emitted on resume).
///
/// Stale down events must not be processed: doing so purges the IP and
/// routes that the boot apply has just configured, and the subsequent up
/// event does not reliably restore them.
fn is_stale_link_down_event(
    event: &InterfaceLinkEvent,
    cur_iface: Option<&Interface>,
) -> bool {
    !event.is_up
        && !event.is_delete
        && cur_iface.is_some_and(|iface| {
            iface.base_iface().link_state == Some(InterfaceLinkState::Up)
        })
}

/// Gather saved routes whose next-hop is the given saved interface.
fn gen_routes_for_iface_up(
    saved_iface: &Interface,
    saved_state: &NetworkState,
) -> Vec<RouteEntry> {
    let mut ret_routes: Vec<RouteEntry> = Vec::new();
    // Include routes to this interface also
    if !saved_iface.is_userspace()
        && let Some(config_rts) = saved_state.routes.config.as_ref()
    {
        for rt in config_rts
            .iter()
            .filter(|rt| is_route_matching_iface(rt, saved_iface))
        {
            ret_routes.push(rt.clone());
        }
    }
    ret_routes
}

fn gen_desired_iface_up(
    saved_iface: &Interface,
    saved_state: &NetworkState,
) -> (Interface, Vec<RouteEntry>) {
    let mut new_iface = saved_iface.clone();
    new_iface.base_iface_mut().state = InterfaceState::Up;
    new_iface.base_iface_mut().auto_connect = None;

    let ret_routes = gen_routes_for_iface_up(saved_iface, saved_state);

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
    // `auto-connect` defaults to `true` when not defined.
    let auto_connect = saved_iface
        .base_iface()
        .auto_connect
        .clone()
        .unwrap_or_default();
    let mut saved_iface = saved_iface.clone();
    saved_iface.base_iface_mut().auto_connect = Some(auto_connect.clone());

    match saved_iface.process_auto_connect(event, &cur_state.ifaces) {
        None => {
            log::trace!("No auto-connect action for {event}");
            None
        }
        Some(false) => {
            let (new_iface, routes) = gen_desired_iface_down(
                &auto_connect,
                &saved_iface,
                saved_state,
            );
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
                gen_desired_iface_up(&saved_iface, saved_state);
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

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use nipart::{
        Interface, InterfaceIpv4, InterfaceLinkEvent, InterfaceState,
        InterfaceType, NetworkState, NipartInterface,
    };

    use super::{
        gen_routes_for_iface_up, handle_event_auto_connect,
        is_route_matching_iface, is_stale_link_down_event,
    };

    fn gen_saved_state() -> NetworkState {
        serde_yaml::from_str(
            r#"---
version: 1
routes:
  config:
  - destination: 0.0.0.0/0
    next-hop-interface: cunet
    next-hop-address: 10.3.221.254
    metric: 100
    table-id: 254
  - destination: 172.25.80.0/24
    next-hop-interface: enp3s0u2u1u2
    next-hop-address: 10.255.0.254
    metric: 103
    table-id: 254
  - destination: 172.25.75.0/24
    next-hop-interface: yellow
    next-hop-address: 10.255.20.254
    metric: 102
    table-id: 254
  - destination: 172.25.81.0/24
    next-hop-interface: yellow
    next-hop-address: 10.255.20.254
    metric: 104
    table-id: 254
  - destination: 172.17.0.0/16
    next-hop-interface: cn
    next-hop-address: 172.17.7.1
    metric: 100
    table-id: 254
interfaces:
- name: cunet
  type: ethernet
  kernel-iface-name: enp3s0u2u1u3c2
  state: up
  profile-name: cunet
  identifier: mac-address
  mac-address: 9C:69:D3:73:03:AC
- name: red
  type: ethernet
  kernel-iface-name: enp3s0u2u1u2
  state: up
  profile-name: red
  identifier: mac-address
  mac-address: 3C:E1:A1:BF:D8:4D
- name: yellow
  type: ethernet
  kernel-iface-name: enp3s0u2u1u4
  state: up
  profile-name: yellow
  identifier: mac-address
  mac-address: F8:C9:03:00:1E:FC
"#,
        )
        .unwrap()
    }

    fn find_iface<'a>(state: &'a NetworkState, name: &str) -> &'a Interface {
        state.ifaces.iter().find(|i| i.name() == name).unwrap()
    }

    #[test]
    fn test_route_matching_by_profile_name() {
        let state = gen_saved_state();
        let cunet = find_iface(&state, "cunet");
        let routes = gen_routes_for_iface_up(cunet, &state);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].destination.as_deref(), Some("0.0.0.0/0"));
    }

    #[test]
    fn test_route_matching_by_kernel_iface_name() {
        let state = gen_saved_state();
        let red = find_iface(&state, "red");
        let routes = gen_routes_for_iface_up(red, &state);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].destination.as_deref(), Some("172.25.80.0/24"));
    }

    #[test]
    fn test_route_matching_by_iface_name() {
        let state = gen_saved_state();
        let yellow = find_iface(&state, "yellow");
        let routes = gen_routes_for_iface_up(yellow, &state);
        let mut dests: Vec<_> = routes
            .iter()
            .filter_map(|rt| rt.destination.as_deref())
            .collect();
        dests.sort_unstable();
        assert_eq!(dests, vec!["172.25.75.0/24", "172.25.81.0/24"]);
    }

    #[test]
    fn test_route_not_matching_iface_excluded() {
        let state = gen_saved_state();
        for name in ["cunet", "red", "yellow"] {
            let iface = find_iface(&state, name);
            assert!(
                !gen_routes_for_iface_up(iface, &state)
                    .iter()
                    .any(
                        |rt| rt.destination.as_deref() == Some("172.17.0.0/16")
                    ),
                "{name} should not pick up the cn route"
            );
        }
    }

    #[test]
    fn test_is_route_matching_iface() {
        let state = gen_saved_state();
        let cunet = find_iface(&state, "cunet");
        let cn_rt = state
            .routes
            .config
            .as_ref()
            .unwrap()
            .iter()
            .find(|rt| rt.destination.as_deref() == Some("172.17.0.0/16"))
            .unwrap();
        assert!(!is_route_matching_iface(cn_rt, cunet));
    }

    fn gen_link_event(iface_name: &str, is_up: bool) -> InterfaceLinkEvent {
        InterfaceLinkEvent {
            iface_name: iface_name.to_string(),
            iface_index: 18,
            iface_type: InterfaceType::Ethernet,
            is_up,
            is_delete: false,
            time_stamp: SystemTime::now(),
            ssid: None,
        }
    }

    #[test]
    fn test_stale_link_down_event_skipped_when_current_up() {
        // A down event processed while the interface is already up is a
        // leftover of the boot-time transient state: it must be skipped so
        // the boot apply result (IP + routes) is not torn down.
        let saved_state = gen_saved_state();
        let cunet = find_iface(&saved_state, "cunet");
        let mut cur_iface = cunet.clone();
        cur_iface.base_iface_mut().link_state =
            Some(nipart::InterfaceLinkState::Up);

        assert!(is_stale_link_down_event(
            &gen_link_event("enp3s0u2u1u3c2", false),
            Some(&cur_iface)
        ));
    }

    #[test]
    fn test_link_down_event_processed_when_current_down() {
        // A real link-down event: the current kernel link state is down, so
        // the event reflects a genuine state change and must be processed
        // (purge IP and routes).
        let saved_state = gen_saved_state();
        let cunet = find_iface(&saved_state, "cunet");
        let mut cur_iface = cunet.clone();
        cur_iface.base_iface_mut().link_state =
            Some(nipart::InterfaceLinkState::Down);

        assert!(!is_stale_link_down_event(
            &gen_link_event("enp3s0u2u1u3c2", false),
            Some(&cur_iface)
        ));
    }

    #[test]
    fn test_up_event_never_stale() {
        // Up events always go through: they are the mechanism to (re)apply
        // the saved config, and skipping them would break hotplug (e.g.
        // wifi association or veth re-plug).
        let saved_state = gen_saved_state();
        let cunet = find_iface(&saved_state, "cunet");
        let mut cur_iface = cunet.clone();
        cur_iface.base_iface_mut().link_state =
            Some(nipart::InterfaceLinkState::Up);

        assert!(!is_stale_link_down_event(
            &gen_link_event("enp3s0u2u1u3c2", true),
            Some(&cur_iface)
        ));
        // Interface already gone: delete event is handled separately.
        assert!(!is_stale_link_down_event(
            &gen_link_event("enp3s0u2u1u3c2", false),
            None
        ));
    }

    #[test]
    fn test_auto_connect_defaults_to_true_on_link_up() {
        // Interface without `auto-connect` defaults to `auto-connect: true`,
        // hence link up should apply the interface along with its routes.
        let saved_state = gen_saved_state();
        let cunet = find_iface(&saved_state, "cunet");
        let event = gen_link_event("enp3s0u2u1u3c2", true);

        let (new_iface, routes) = handle_event_auto_connect(
            &event,
            cunet,
            &saved_state,
            &NetworkState::default(),
        )
        .expect("auto-connect defaults to true");

        assert_eq!(new_iface.name(), "cunet");
        assert_eq!(new_iface.base_iface().state, InterfaceState::Up);
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].destination.as_deref(), Some("0.0.0.0/0"));
    }

    #[test]
    fn test_auto_connect_defaults_to_true_on_link_down() {
        // On link down, the default auto-connect purges IP and marks routes
        // absent, but does not bring the interface down.
        let saved_state = gen_saved_state();
        let cunet = find_iface(&saved_state, "cunet");
        let event = gen_link_event("enp3s0u2u1u3c2", false);

        let (new_iface, routes) = handle_event_auto_connect(
            &event,
            cunet,
            &saved_state,
            &NetworkState::default(),
        )
        .expect("auto-connect defaults to true");

        assert_eq!(new_iface.base_iface().state, InterfaceState::Up);
        assert_eq!(
            new_iface.base_iface().ipv4,
            Some(InterfaceIpv4::new_disabled())
        );
        assert_eq!(routes.len(), 1);
        assert!(routes[0].is_absent());
    }
}
