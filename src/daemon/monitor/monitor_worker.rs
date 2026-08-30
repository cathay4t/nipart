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
use nipart::{ErrorKind, InterfaceLinkEvent, InterfaceType, NipartError};
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
    use std::time::{Duration, Instant};

    use futures_channel::mpsc::unbounded;
    use nipart::{InterfaceLinkEvent, InterfaceType};

    use super::{NipartMonitorWorker, format_mac};
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
}
