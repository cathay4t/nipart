// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, HashSet},
    io::Read,
    time::{Duration, Instant},
};

use futures_channel::{
    mpsc::{UnboundedReceiver, UnboundedSender},
    oneshot::Sender,
};
use futures_util::{SinkExt, StreamExt};
use nipart::{
    ErrorKind, Interface, InterfaceLinkEvent, InterfaceType, NipartError,
    NipartInterface,
};
use rtnetlink::{
    MulticastGroup, new_multicast_connection,
    packet_core::{NetlinkMessage, NetlinkPayload},
    packet_route::{
        RouteNetlinkMessage,
        link::{
            InfoKind, LinkAttribute, LinkFlags, LinkInfo, LinkLayerType,
            LinkMessage, WirelessEvent,
        },
    },
    sys::SocketAddr,
};
use wl_nl80211::{Ieee80211Element, Ieee80211Elements, packet_core::Parseable};

use super::super::{daemon::NipartManagerCmd, task::TaskWorker};

// When the same event happens, when should we consider previous event expired.
const EVENT_EXPIRE_TIME_SEC: u64 = 300;

// When the event changed to down, we wait 10 seconds to prevent flipping
const DOWN_WAIT_SEC: u64 = 10;
// Check delay queue event every second if delay_queue is not empty
const DELAY_TICK_SEC_IF_BUSY: u64 = 1;
// Check delay queue event every day if delay_queue is empty, we cannot use
// Duration::MAX which will cause overflow on Interval::reset_after()
const DELAY_TICK_SEC_IF_FREE: u64 = 24 * 60 * 60;

#[derive(Debug, Clone)]
pub(crate) enum NipartMonitorCmd {
    /// Set the sender for monitor to contact commander. Must be invoked
    /// right after NipartMonitorWorker started.
    SetCommanderSender(UnboundedSender<NipartManagerCmd>),
    /// Start monitoring on specified interface
    AddIface(String),
    /// Stop monitoring on specified interface
    DelIface(String),
    /// Start monitoring on specified MAC address (uppercase, e.g.
    /// `02:00:00:00:00:03`): link events of interfaces carrying this MAC
    /// are emitted even when their kernel name is not known yet (saved
    /// `identifier: mac-address` configs whose NIC is not present at boot).
    AddMacWatch(String),
    /// Stop monitoring on specified MAC address
    DelMacWatch(String),
    /// Start monitoring on WIFI SSID association
    EnableWifiMonitor,
    /// Stop monitoring on WIFI SSID association
    DisableWifiMonitor,
    /// Stop the monitoring but preserving the internal monitoring list
    Pause,
    /// Resume the monitoring, emit current status of monitoring
    /// interface list.
    Resume,
    /// Record that an interface/profile was explicitly brought down by
    /// `npt down`.  Link events of these interfaces must not be forwarded
    /// to the event worker until `npt up` clears the marker.
    MarkExplicitlyDown(Vec<String>),
    /// Forget that an interface/profile was explicitly brought down.
    ClearExplicitlyDown(Vec<String>),
}

impl std::fmt::Display for NipartMonitorCmd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SetCommanderSender(_) => {
                write!(f, "set-commander-sender")
            }
            Self::AddIface(iface) => {
                write!(f, "start-iface-monitor:{iface}")
            }
            Self::DelIface(iface) => {
                write!(f, "stop-iface-monitor:{iface}")
            }
            Self::AddMacWatch(mac) => {
                write!(f, "start-mac-monitor:{mac}")
            }
            Self::DelMacWatch(mac) => {
                write!(f, "stop-mac-monitor:{mac}")
            }
            Self::EnableWifiMonitor => {
                write!(f, "enable-wifi-monitor")
            }
            Self::DisableWifiMonitor => {
                write!(f, "disable-wifi-monitor")
            }
            Self::Pause => {
                write!(f, "pause-monitor")
            }
            Self::Resume => {
                write!(f, "resume-monitor")
            }
            Self::MarkExplicitlyDown(ifaces) => {
                write!(f, "mark-explicitly-down:{ifaces:?}")
            }
            Self::ClearExplicitlyDown(ifaces) => {
                write!(f, "clear-explicitly-down:{ifaces:?}")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NipartMonitorReply {
    None,
}

type FromManager = (
    NipartMonitorCmd,
    Sender<Result<NipartMonitorReply, NipartError>>,
);

#[derive(Debug)]
pub(crate) struct NipartMonitorWorker {
    receiver: UnboundedReceiver<FromManager>,
    netlink_handle: Option<rtnetlink::Handle>,
    netlink_msg_receiver: Option<
        UnboundedReceiver<(NetlinkMessage<RouteNetlinkMessage>, SocketAddr)>,
    >,
    iface_monitor_list: HashSet<String>,
    // MAC addresses (uppercase) of saved `identifier: mac-address` configs
    // whose NIC is not present yet: when a NIC carrying one of these MACs
    // appears, its link events are emitted so the event worker can apply
    // the saved config.
    mac_watch_list: HashSet<String>,
    // Latest MAC address (uppercase) observed per kernel interface name,
    // used to match link events against `mac_watch_list`.
    iface_mac: HashMap<String, String>,
    wifi_monitor_enabled: bool,
    msg_to_commander: Option<UnboundedSender<NipartManagerCmd>>,
    manual_paused: bool,
    /// Interface/profile aliases explicitly brought down by `npt down`.
    /// Link events for these interfaces are dropped at the monitor worker so
    /// the event worker cannot re-apply the saved config and its routes.
    explicitly_down: HashSet<String>,
    emited: HashMap<String, InterfaceLinkEvent>,
    delay_queue: HashMap<String, (InterfaceLinkEvent, Instant)>,
}

impl TaskWorker for NipartMonitorWorker {
    type Cmd = NipartMonitorCmd;
    type Reply = NipartMonitorReply;

    async fn new(
        receiver: UnboundedReceiver<FromManager>,
    ) -> Result<Self, NipartError> {
        Ok(Self {
            receiver,
            iface_monitor_list: HashSet::new(),
            mac_watch_list: HashSet::new(),
            iface_mac: HashMap::new(),
            wifi_monitor_enabled: false,
            netlink_handle: None,
            netlink_msg_receiver: None,
            manual_paused: false,
            msg_to_commander: None,
            explicitly_down: HashSet::new(),
            emited: HashMap::new(),
            delay_queue: HashMap::new(),
        })
    }

    fn receiver(&mut self) -> &mut UnboundedReceiver<FromManager> {
        &mut self.receiver
    }

    async fn process_cmd(
        &mut self,
        cmd: NipartMonitorCmd,
    ) -> Result<NipartMonitorReply, NipartError> {
        log::debug!("Processing monitor command: {cmd}");
        match cmd {
            NipartMonitorCmd::SetCommanderSender(sender) => {
                self.msg_to_commander = Some(sender);
            }
            NipartMonitorCmd::AddIface(iface) => {
                self.iface_monitor_list.insert(iface);
                if self.should_start_netlink() {
                    self.resume().await?;
                }
            }
            NipartMonitorCmd::DelIface(iface) => {
                self.iface_monitor_list.remove(&iface);
                if self.should_pause() {
                    self.pause();
                }
            }
            NipartMonitorCmd::AddMacWatch(mac) => {
                self.mac_watch_list.insert(mac.to_ascii_uppercase());
                if self.should_start_netlink() {
                    self.resume().await?;
                }
            }
            NipartMonitorCmd::DelMacWatch(mac) => {
                self.mac_watch_list.remove(&mac.to_ascii_uppercase());
                if self.should_pause() {
                    self.pause();
                }
            }
            NipartMonitorCmd::EnableWifiMonitor => {
                self.wifi_monitor_enabled = true;
                if self.should_start_netlink() {
                    self.resume().await?;
                }
            }
            NipartMonitorCmd::DisableWifiMonitor => {
                self.wifi_monitor_enabled = false;
                if self.should_pause() {
                    self.pause();
                }
            }
            NipartMonitorCmd::Pause => {
                self.manual_paused = true;
                self.pause();
            }
            NipartMonitorCmd::Resume => {
                self.manual_paused = false;
                if self.should_resume() && self.should_start_netlink() {
                    self.resume().await?;
                }
            }
            NipartMonitorCmd::MarkExplicitlyDown(names) => {
                self.explicitly_down.extend(names);
            }
            NipartMonitorCmd::ClearExplicitlyDown(names) => {
                for name in names {
                    self.explicitly_down.remove(&name);
                }
            }
        }
        Ok(NipartMonitorReply::None)
    }

    async fn run(&mut self) {
        let mut ticker =
            tokio::time::interval(Duration::from_secs(DELAY_TICK_SEC_IF_BUSY));
        // First tick happen immediately
        ticker.tick().await;
        loop {
            if self.delay_queue.is_empty() {
                ticker.reset_after(Duration::from_secs(DELAY_TICK_SEC_IF_FREE));
            } else {
                ticker.reset_after(Duration::from_secs(DELAY_TICK_SEC_IF_BUSY));
            }
            if let Some(mut netlink_msg_receiver) =
                self.netlink_msg_receiver.take()
            {
                tokio::select! {
                    cmd_result = self.recv_cmd() => {
                        if let Some((cmd, sender)) = cmd_result {
                            let cmd_str = cmd.to_string();
                            let result = self.process_cmd(cmd).await;
                            if sender.send(result).is_err() {
                                log::error!(
                                    "Failed to send reply for command {cmd_str}"
                                );
                            }
                        } else {
                            break;
                        }
                    }
                    result = netlink_msg_receiver.next() => {
                        if let Some((nl_msg, _)) = result
                            && let Err(e) = self.process_rtnl_message(
                                nl_msg,
                            ).await {
                                log::error!("{e}");
                            }
                    }
                    _ = ticker.tick() => {
                        if let Err(e) = self.process_delay_queue().await {
                            log::error!("{e}");
                        }
                    }
                }
                if !self.manual_paused {
                    self.netlink_msg_receiver = Some(netlink_msg_receiver);
                }
            } else if let Some((cmd, sender)) = self.recv_cmd().await {
                let cmd_str = cmd.to_string();
                let result = self.process_cmd(cmd).await;
                if sender.send(result).is_err() {
                    log::error!("Failed to send reply for command {cmd_str}");
                }
            } else {
                break;
            }
        }
    }
}

impl NipartMonitorWorker {
    /// Whether the netlink socket should be dropped: no interface, no MAC
    /// watch and no wifi monitoring left.
    fn should_pause(&self) -> bool {
        self.iface_monitor_list.is_empty()
            && self.mac_watch_list.is_empty()
            && !self.wifi_monitor_enabled
    }

    /// Whether the netlink socket should be (re)created: at least one
    /// interface, one MAC watch or wifi monitoring is active.
    fn should_resume(&self) -> bool {
        !self.iface_monitor_list.is_empty()
            || !self.mac_watch_list.is_empty()
            || self.wifi_monitor_enabled
    }

    /// Whether a new netlink multicast connection should be created.
    ///
    /// `netlink_msg_receiver` is temporarily moved out of the worker while
    /// `run()` polls it with `tokio::select!`, so it cannot be used as the
    /// "socket is active" check from `process_cmd()`. The handle remains
    /// set for the whole active period.
    fn should_start_netlink(&self) -> bool {
        !self.manual_paused && self.netlink_handle.is_none()
    }

    fn pause(&mut self) {
        self.netlink_handle = None;
        self.netlink_msg_receiver = None;
        // The monitor session is over: stale last-event/MAC observations
        // from a previous session must not suppress the fresh link dump
        // emitted after the next resume (e.g. the same wifi-phy up event
        // seen again after a daemon-managed reconnect).
        self.emited.clear();
        self.delay_queue.clear();
        self.iface_mac.clear();
    }

    async fn notify(
        &mut self,
        event: InterfaceLinkEvent,
    ) -> Result<(), NipartError> {
        log::trace!("NipartMonitorWorker sending out {event:?}");
        if let Some(sender) = self.msg_to_commander.as_mut() {
            let cmd = NipartManagerCmd::LinkEvent(Box::new(event.clone()));
            sender.send(cmd).await.map_err(|e| {
                NipartError::new(
                    ErrorKind::Bug,
                    format!(
                        "NipartMonitorWorker: Failed to send to commander: {e}"
                    ),
                )
            })?;
            // Remove event on delay_queue also
            self.delay_queue.remove(&event.iface_name);
            if event.is_delete {
                self.emited.remove(&event.iface_name);
            } else {
                self.emited.insert(event.iface_name.to_string(), event);
            }
            Ok(())
        } else {
            Err(NipartError::new(
                ErrorKind::Bug,
                format!(
                    "Got NipartMonitorWorker without msg_to_commander: \
                     {self:?}"
                ),
            ))
        }
    }

    async fn process_delay_queue(&mut self) -> Result<(), NipartError> {
        // holding processed interface names
        let mut pending_changes = Vec::new();
        for (iface_name, (_, time)) in self.delay_queue.iter() {
            if time < &Instant::now() {
                pending_changes.push(iface_name.to_string());
            }
        }
        for iface_name in pending_changes {
            log::trace!("Emit delayed event on {iface_name}");
            if let Some((event, _)) = self.delay_queue.remove(&iface_name) {
                if event_is_explicitly_down(&event, &self.explicitly_down) {
                    log::trace!(
                        "Ignoring delayed link event {event}: interface was \
                         explicitly brought down by `npt down`"
                    );
                    continue;
                }
                if let Some(previous_event) =
                    self.emited.get(event.iface_name.as_str())
                    && previous_event.is_up
                    && event.is_up
                {
                    log::trace!("Link is already up, no need to emit event");
                } else {
                    self.notify(event).await?;
                }
            }
        }
        Ok(())
    }

    fn delay_notify(&mut self, event: InterfaceLinkEvent, time: Duration) {
        log::trace!("NipartMonitorWorker delay notify {event:?}");
        self.delay_queue
            .insert(event.iface_name.clone(), (event, Instant::now() + time));
    }

    async fn resume(&mut self) -> Result<(), NipartError> {
        let (conn, handle, msg) =
            new_multicast_connection(&[MulticastGroup::Link]).map_err(|e| {
                NipartError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "Failed to create netlink multicast socket for \
                         interface monitor: {e}"
                    ),
                )
            })?;
        tokio::spawn(conn);

        let mut link_handle = handle.link().get().execute();
        while let Some(Ok(link_msg)) = link_handle.next().await {
            if let Some((event, mac)) =
                parse_link_msg(&link_msg, self.wifi_monitor_enabled, false)
            {
                if let Some(mac) = mac {
                    self.iface_mac.insert(event.iface_name.clone(), mac);
                }
                self.try_notify(event).await?;
            }
        }

        self.netlink_handle = Some(handle);
        self.netlink_msg_receiver = Some(msg);
        Ok(())
    }

    async fn process_rtnl_message(
        &mut self,
        nl_msg: NetlinkMessage<RouteNetlinkMessage>,
    ) -> Result<(), NipartError> {
        if let Some((event, mac)) =
            parse_route_netlink_msg(nl_msg, self.wifi_monitor_enabled)
        {
            if let Some(mac) = mac {
                self.iface_mac.insert(event.iface_name.clone(), mac);
            }
            self.try_notify(event).await?;
        }
        Ok(())
    }

    fn event_is_interested(&self, event: &InterfaceLinkEvent) -> bool {
        self.iface_monitor_list.contains(&event.iface_name)
            // A NIC matching a saved `identifier: mac-address` config may
            // carry a kernel name unknown to us (it was not present at boot):
            // match it by the MAC address instead.
            || self
                .iface_mac
                .get(&event.iface_name)
                .is_some_and(|mac| self.mac_watch_list.contains(mac))
            || (self.wifi_monitor_enabled && event.ssid.is_some())
            || (self.wifi_monitor_enabled
                && event.iface_type == InterfaceType::WifiPhy)
    }

    async fn try_notify(
        &mut self,
        event: InterfaceLinkEvent,
    ) -> Result<(), NipartError> {
        if event_is_explicitly_down(&event, &self.explicitly_down) {
            log::trace!(
                "Ignoring link event {event}: interface was explicitly \
                 brought down by `npt down`"
            );
            return Ok(());
        }
        if !self.event_is_interested(&event) {
            log::trace!("Event {event} is not interested");
            return Ok(());
        }

        // When SSID changes, normally it should go through down->up chain,
        // hence no need to handle specially here.  WIFI up notifications
        // carry the SSID in their `WirelessEvent` attribute (see
        // `parse_link_msg()`), so an up event without SSID is either a
        // non-association link event or a duplicate; the normal down->up
        // deduplication below handles both.
        if event.is_delete {
            // delete event, emit now.
            self.notify(event).await?;
        } else if let Some(previous_event) =
            self.emited.get(event.iface_name.as_str())
        {
            if let Ok(elapsed) = previous_event.time_stamp.elapsed()
                && elapsed > Duration::from_secs(EVENT_EXPIRE_TIME_SEC)
            {
                // If previous event expired, emit now.
                self.notify(event).await?;
            } else if !previous_event.is_up && event.is_up {
                // If change from down to up, emit now.
                self.notify(event).await?;
            } else {
                // delay emit
                self.delay_notify(event, Duration::from_secs(DOWN_WAIT_SEC));
            }
        } else {
            // If no previous event, emit now.
            self.notify(event).await?;
        }
        Ok(())
    }
}

/// All names which may identify an interface/profile: the logical name,
/// kernel interface name and profile name.  `npt down` may be invoked with
/// any of them, and link events only carry the kernel interface name.
pub(crate) fn iface_identity_names(iface: &Interface) -> Vec<String> {
    let mut names = vec![iface.name().to_string()];
    let kernel_iface_name = iface.kernel_iface_name();
    if !kernel_iface_name.is_empty() && kernel_iface_name != iface.name() {
        names.push(kernel_iface_name.to_string());
    }
    if let Some(profile_name) = iface.base_iface().profile_name.as_deref()
        && profile_name != iface.name()
        && profile_name != kernel_iface_name
    {
        names.push(profile_name.to_string());
    }
    names
}

fn event_is_explicitly_down(
    event: &InterfaceLinkEvent,
    explicitly_down: &HashSet<String>,
) -> bool {
    explicitly_down.contains(&event.iface_name)
        || event
            .ssid
            .as_deref()
            .is_some_and(|ssid| explicitly_down.contains(ssid))
}

fn parse_link_msg(
    link_msg: &LinkMessage,
    wifi_monitor_enabled: bool,
    is_delete: bool,
) -> Option<(InterfaceLinkEvent, Option<String>)> {
    let iface_name = link_msg.attributes.iter().find_map(|attr| {
        if let &LinkAttribute::IfName(iface_name) = &attr {
            Some(iface_name.to_string())
        } else {
            None
        }
    })?;
    let iface_index = link_msg.header.index;
    // The MAC address of the interface, used to match link events against
    // saved `identifier: mac-address` configs whose NIC was not present at
    // boot (their kernel name is unknown until the NIC appears).
    let mac = link_msg.attributes.iter().find_map(|attr| {
        if let LinkAttribute::Address(addr) = attr {
            format_mac(addr)
        } else {
            None
        }
    });
    // TODO: We should return early when event should be ignored(up event for up
    // link, or down event for down link, etc).

    let mut iface_type = parse_iface_type_from_nl_msg(link_msg);
    // The rtnetlink protocol has no information about wireless, so wireless
    // NIC is treated as InterfaceType::Ethernet in rtnetlink.
    if iface_type == InterfaceType::Ethernet && is_wifi_phy_nic(&iface_name) {
        iface_type = InterfaceType::WifiPhy;
    }

    let mut event = InterfaceLinkEvent::new(
        iface_name.clone(),
        iface_index,
        iface_type,
        false,
        None,
    );

    if is_delete {
        event.is_delete = true;
        return Some((event, mac));
    }

    // Unlike `IFLA_OPERSTATE`, the `IFF_*` flags are present in every
    // RTM_NEWLINK message, including the `IFLA_WIRELESS`-only notifications
    // emitted by `wireless_send_event()` on WIFI association. Those
    // notifications carry the SSID in their `WirelessEvent` attribute, so we
    // must accept them to get an up event with SSID included.
    //
    // Use `IFF_LOWER_UP` (carrier up) as the primary signal: on association,
    // `wireless_send_event()` runs after `netif_carrier_on()` but before
    // linkwatch promotes `operstate`, so the SSID-bearing notification has
    // `IFF_LOWER_UP` but not yet `IFF_RUNNING`. Keep `IFF_RUNNING` as well to
    // also accept the "operational state UP/UNKNOWN" notifications used by
    // notification-less drivers (see the DHCP `wait_link_carrier` fix).
    event.is_up = link_msg.header.flags.contains(LinkFlags::LowerUp)
        || link_msg.header.flags.contains(LinkFlags::Running);

    // `wireless_send_event()` sends RTM_NEWLINK with only `IFLA_IFNAME`
    // and `IFLA_WIRELESS`. Only the association IE events carry the SSID
    // of a new association; everything else (e.g. the `SIOCGIWSCAN`
    // scan-done event emitted after every scan) is wireless telemetry,
    // not a link-state change. Dropping those here prevents a background
    // roam scan from re-applying the saved config and restarting DHCP.
    if should_ignore_wireless_notification(link_msg) {
        log::trace!(
            "{iface_name}: ignoring wireless-only RTM_NEWLINK notification \
             without association IEs"
        );
        return None;
    }

    if wifi_monitor_enabled && event.iface_type == InterfaceType::WifiPhy {
        let Some(wifi_ie) = link_msg.attributes.iter().find_map(|attr| {
            if let LinkAttribute::Wireless(wifi_attr) = attr {
                match wifi_attr {
                    WirelessEvent::AssociateResponse(wifi_ie)
                    | WirelessEvent::AssociateRequest(wifi_ie) => Some(wifi_ie),
                    _ => {
                        log::trace!(
                            "{iface_name}: Got unknown \
                             LinkAttribute::Wireless attribute {wifi_attr:?}"
                        );
                        None
                    }
                }
            } else {
                None
            }
        }) else {
            // If we cannot get SSID out of wifi-phy event, we still try to
            // emit it; the event worker checks the current interface state
            // for SSID when processing the event.
            log::trace!(
                "{iface_name}: No SSID out wifi-phy event, event worker will \
                 resolve it from current interface state"
            );
            return Some((event, mac));
        };

        match Ieee80211Elements::parse(wifi_ie.as_slice()) {
            Ok(elements) => {
                log::trace!("{iface_name}: Got WIFI IE: {elements:?}");
                for ie in elements.0.into_iter() {
                    if let Ieee80211Element::Ssid(ssid) = ie
                        && !ssid.is_empty()
                    {
                        event.ssid = Some(ssid);
                        break;
                    }
                }
                if event.is_up && event.ssid.is_none() {
                    log::trace!(
                        "{iface_name}: wifi-phy up event without SSID in up \
                         netlink message"
                    );
                }
            }
            Err(e) => {
                log::trace!(
                    "{iface_name}: unknown wifi information element: {e} \
                     {wifi_ie:?})"
                );
            }
        }
    }

    Some((event, mac))
}

/// Format a raw MAC address (6 bytes) into the uppercase
/// `XX:XX:XX:XX:XX:XX` form used by the saved config and the MAC watch
/// list.  Returns `None` for addresses of any other length (e.g. the
/// 20-byte InfiniBand addresses) which cannot match an ethernet MAC.
fn format_mac(addr: &[u8]) -> Option<String> {
    if addr.len() != 6 {
        return None;
    }
    Some(
        addr.iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

fn parse_route_netlink_msg(
    nl_msg: NetlinkMessage<RouteNetlinkMessage>,
    wifi_monitor_enabled: bool,
) -> Option<(InterfaceLinkEvent, Option<String>)> {
    if let NetlinkPayload::InnerMessage(RouteNetlinkMessage::NewLink(
        link_msg,
    )) = nl_msg.payload
    {
        parse_link_msg(&link_msg, wifi_monitor_enabled, false)
    } else if let NetlinkPayload::InnerMessage(RouteNetlinkMessage::DelLink(
        link_msg,
    )) = nl_msg.payload
    {
        parse_link_msg(&link_msg, wifi_monitor_enabled, true)
    } else {
        log::trace!("BUG: Unexpected rtnetlink notification msg: {nl_msg:?}");
        None
    }
}

/// Whether an `RTM_NEWLINK` message is a wireless-only notification
/// emitted by the kernel's `wireless_send_event()` (attributes are only
/// `IFLA_IFNAME` + `IFLA_WIRELESS`) and does not carry association IEs.
///
/// `wireless_send_event()` is also used for non-association telemetry such
/// as the `SIOCGIWSCAN` scan-done event emitted after every scan; those
/// messages are not link-state changes and must not be converted into a
/// link event, otherwise a background roam scan would re-apply the saved
/// config and restart DHCP.
fn should_ignore_wireless_notification(link_msg: &LinkMessage) -> bool {
    let mut has_wireless_attr = false;
    let mut has_association_ie = false;
    let mut has_other_attr = false;
    for attr in &link_msg.attributes {
        match attr {
            LinkAttribute::IfName(_) => {}
            LinkAttribute::Wireless(wifi_attr) => {
                has_wireless_attr = true;
                has_association_ie |= is_wifi_association_event(wifi_attr);
            }
            _ => has_other_attr = true,
        }
    }
    has_wireless_attr && !has_other_attr && !has_association_ie
}

/// Whether the wireless event is an association IE notification carrying
/// the SSID of the new association.
fn is_wifi_association_event(wifi_attr: &WirelessEvent) -> bool {
    matches!(
        wifi_attr,
        WirelessEvent::AssociateRequest(_)
            | WirelessEvent::AssociateResponse(_)
    )
}

fn parse_iface_type_from_nl_msg(link_msg: &LinkMessage) -> InterfaceType {
    if let Some(link_infos) = link_msg.attributes.iter().find_map(|attr| {
        if let LinkAttribute::LinkInfo(infos) = attr {
            Some(infos)
        } else {
            None
        }
    }) && let Some(info_kind) = link_infos.iter().find_map(|info| {
        if let LinkInfo::Kind(k) = info {
            Some(k)
        } else {
            None
        }
    }) {
        match info_kind {
            InfoKind::Bond => InterfaceType::Bond,
            InfoKind::Veth => InterfaceType::Veth,
            InfoKind::Bridge => InterfaceType::LinuxBridge,
            InfoKind::Vlan => InterfaceType::Vlan,
            InfoKind::Vxlan => InterfaceType::Vxlan,
            InfoKind::Dummy => InterfaceType::Dummy,
            InfoKind::Tun => InterfaceType::Tun,
            InfoKind::Vrf => InterfaceType::Vrf,
            InfoKind::MacVlan => InterfaceType::MacVlan,
            InfoKind::MacVtap => InterfaceType::MacVtap,
            InfoKind::Ipoib => InterfaceType::InfiniBand,
            InfoKind::IpVlan => InterfaceType::IpVlan,
            InfoKind::MacSec => InterfaceType::MacSec,
            InfoKind::Hsr => InterfaceType::Hsr,
            InfoKind::Xfrm => InterfaceType::Xfrm,
            v => InterfaceType::Unknown(v.to_string().to_lowercase()),
        }
    } else {
        match link_msg.header.link_layer_type {
            LinkLayerType::Ether => InterfaceType::Ethernet,
            LinkLayerType::Loopback => InterfaceType::Loopback,
            LinkLayerType::Infiniband => InterfaceType::InfiniBand,
            v => InterfaceType::Unknown(v.to_string().to_lowercase()),
        }
    }
}

/// Systemd udev is using `/sys/class/net/{iface_name}/uevent` content
/// `DEVTYPE=wlan` to determine whether wireless or not.
/// And linux kernel code `SET_NETDEV_DEVTYPE(dev, &wiphy_type)` also confirmed
/// so.
fn is_wifi_phy_nic(iface_name: &str) -> bool {
    let mut content = String::new();

    if let Ok(mut fd) =
        std::fs::File::open(format!("/sys/class/net/{iface_name}/uevent"))
        && fd.read_to_string(&mut content).is_ok()
    {
        content.contains("DEVTYPE=wlan")
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        time::{Duration, Instant},
    };

    use futures_channel::mpsc::unbounded;
    use nipart::{Interface, InterfaceLinkEvent, InterfaceType};
    use rtnetlink::{
        packet_core::{Emitable, Parseable},
        packet_route::link::{
            LinkAttribute, LinkHeader, LinkLayerType, LinkMessage,
            WirelessEvent,
        },
    };

    use super::{
        NipartMonitorWorker, event_is_explicitly_down, format_mac,
        iface_identity_names, should_ignore_wireless_notification,
    };
    use crate::task::TaskWorker;

    fn gen_event(iface_name: &str) -> InterfaceLinkEvent {
        InterfaceLinkEvent::new(
            iface_name.to_string(),
            10,
            InterfaceType::Ethernet,
            true,
            None,
        )
    }

    fn gen_worker() -> NipartMonitorWorker {
        let (_tx, rx) = unbounded();
        tokio::runtime::Runtime::new()
            .expect("Failed to create tokio runtime")
            .block_on(NipartMonitorWorker::new(rx))
            .expect("Failed to create monitor worker")
    }

    fn gen_wireless_link_msg(
        wifi_attr: WirelessEvent,
        with_other_attr: bool,
    ) -> LinkMessage {
        let header = LinkHeader {
            index: 2,
            link_layer_type: LinkLayerType::Ether,
            ..Default::default()
        };
        let mut attrs = vec![
            LinkAttribute::IfName("wlan0".to_string()),
            LinkAttribute::Wireless(wifi_attr),
        ];
        if with_other_attr {
            attrs.push(LinkAttribute::Address(vec![
                0x02, 0x00, 0x00, 0x00, 0x00, 0x01,
            ]));
        }
        let mut buf =
            vec![0; header.buffer_len() + attrs.as_slice().buffer_len()];
        header.emit(&mut buf);
        attrs.as_slice().emit(&mut buf[header.buffer_len()..]);
        LinkMessage::parse(&buf).expect("Failed to parse link message")
    }

    #[test]
    fn test_format_mac() {
        assert_eq!(
            format_mac(&[0x02, 0x00, 0x00, 0x00, 0x00, 0x10]),
            Some("02:00:00:00:00:10".to_string())
        );
        // Addresses of other lengths (e.g. InfiniBand 20 bytes) cannot
        // match an ethernet MAC.
        assert_eq!(format_mac(&[0x00, 0x11]), None);
        assert_eq!(format_mac(&[]), None);
    }

    #[test]
    fn test_ignore_wireless_only_scan_done_notification() {
        // The kernel emits an `IFLA_WIRELESS`-only RTM_NEWLINK carrying
        // `struct iw_event { len=16, cmd=SIOCGIWSCAN(0x8B19) }` when a
        // scan finishes. It is not a link-state change and must not be
        // turned into a link-up event (which would re-apply the saved
        // wifi config and restart DHCP).
        let scan_done = WirelessEvent::Other(vec![
            16, 0, 25, 139, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        assert!(should_ignore_wireless_notification(&gen_wireless_link_msg(
            scan_done.clone(),
            false
        )));

        // A real link message carrying other attributes (e.g. a link dump)
        // is kept even when it also carries a non-association wireless
        // attribute.
        assert!(!should_ignore_wireless_notification(
            &gen_wireless_link_msg(scan_done, true)
        ));
    }

    #[test]
    fn test_keep_wireless_association_ie_notification() {
        // Association IE notifications are wireless-only but carry the
        // SSID of the new association and must be kept.
        let link_msg = gen_wireless_link_msg(
            WirelessEvent::AssociateResponse(vec![
                // SSID IE: element id 0, length 8, "Test-WIFI"
                0, 8, b'T', b'e', b's', b't', b'-', b'W', b'I', b'F', b'I',
            ]),
            false,
        );
        assert!(!should_ignore_wireless_notification(&link_msg));
    }

    #[test]
    fn test_event_is_interested_by_mac_watch() {
        // A NIC matching a saved `identifier: mac-address` config carries a
        // kernel name unknown to the monitor: the event must be emitted
        // when its MAC address is watched.
        let mut worker = gen_worker();
        worker
            .mac_watch_list
            .insert("02:00:00:00:00:10".to_string());
        worker
            .iface_mac
            .insert("enp4s0".to_string(), "02:00:00:00:00:10".to_string());

        assert!(worker.event_is_interested(&gen_event("enp4s0")));

        // Same interface name with a different (unwatched) MAC: not
        // interested unless the name itself is monitored.
        worker
            .iface_mac
            .insert("enp4s0".to_string(), "02:00:00:00:00:03".to_string());
        assert!(!worker.event_is_interested(&gen_event("enp4s0")));

        // Interface without any observed MAC address.
        worker.iface_mac.remove("enp4s0");
        assert!(!worker.event_is_interested(&gen_event("enp4s0")));
    }

    #[test]
    fn test_event_is_interested_by_name_or_wifi() {
        let mut worker = gen_worker();
        // Monitored by kernel name.
        worker.iface_monitor_list.insert("enp1s0".to_string());
        assert!(worker.event_is_interested(&gen_event("enp1s0")));
        assert!(!worker.event_is_interested(&gen_event("enp2s0")));

        // Wifi monitoring passes all wifi-phy events.
        worker.wifi_monitor_enabled = true;
        let wifi_event = InterfaceLinkEvent::new(
            "wlan0".to_string(),
            10,
            InterfaceType::WifiPhy,
            true,
            None,
        );
        assert!(worker.event_is_interested(&wifi_event));
        assert!(!worker.event_is_interested(&gen_event("enp2s0")));
    }

    #[test]
    fn test_should_pause_and_resume_include_mac_watch() {
        let mut worker = gen_worker();
        assert!(worker.should_pause());
        assert!(!worker.should_resume());

        // A MAC watch alone keeps the netlink socket alive.
        worker
            .mac_watch_list
            .insert("02:00:00:00:00:10".to_string());
        assert!(!worker.should_pause());
        assert!(worker.should_resume());

        // Removing the last watch pauses again.
        worker.mac_watch_list.clear();
        assert!(worker.should_pause());
    }

    #[test]
    fn test_pause_clears_monitor_session_state() {
        let mut worker = gen_worker();
        worker
            .emited
            .insert("enp1s0".to_string(), gen_event("enp1s0"));
        worker.delay_queue.insert(
            "enp2s0".to_string(),
            (
                gen_event("enp2s0"),
                Instant::now() + Duration::from_secs(10),
            ),
        );
        worker
            .iface_mac
            .insert("enp3s0".to_string(), "02:00:00:00:00:03".to_string());

        worker.pause();

        assert!(worker.emited.is_empty());
        assert!(worker.delay_queue.is_empty());
        assert!(worker.iface_mac.is_empty());
    }

    #[test]
    fn test_pause_keeps_explicit_down_list() {
        // The explicit-down marker must survive the monitor pause/resume
        // cycle around `npt down`, otherwise the link dump emitted on resume
        // would re-apply the saved config.
        let mut worker = gen_worker();
        worker.explicitly_down.insert("enp1s0".to_string());

        worker.pause();

        assert!(worker.explicitly_down.contains("enp1s0"));
    }

    #[test]
    fn test_iface_identity_names_include_profile_and_kernel_names() {
        let iface: Interface = rmsd_yaml::from_str(
            r#"---
            name: eth9
            kernel-iface-name: eth9
            profile-name: wan9
            type: ethernet
            state: up
            "#,
        )
        .unwrap();

        let names = iface_identity_names(&iface);
        assert!(names.iter().any(|name| name == "eth9"));
        assert!(names.iter().any(|name| name == "wan9"));
    }

    #[test]
    fn test_event_is_explicitly_down_matches_iface_and_ssid() {
        let explicitly_down =
            HashSet::from(["eth9".to_string(), "Test-WIFI".to_string()]);

        let iface_event = gen_event("eth9");
        assert!(event_is_explicitly_down(&iface_event, &explicitly_down));

        let mut wifi_event = gen_event("wlan0");
        wifi_event.ssid = Some("Test-WIFI".to_string());
        assert!(event_is_explicitly_down(&wifi_event, &explicitly_down));

        assert!(!event_is_explicitly_down(
            &gen_event("enp1s0"),
            &explicitly_down
        ));
    }
}
