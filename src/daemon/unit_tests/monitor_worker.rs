// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use futures_channel::mpsc::unbounded;
use nipart::{Interface, InterfaceLinkEvent, InterfaceType};
use rtnetlink::{
    packet_core::{Emitable, Parseable},
    packet_route::link::{
        LinkAttribute, LinkHeader, LinkLayerType, LinkMessage, WirelessEvent,
    },
};

use super::{
    NipartMonitorCmd, NipartMonitorWorker, event_is_explicitly_down,
    format_mac, iface_identity_names, should_ignore_wireless_notification,
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
    let mut buf = vec![0; header.buffer_len() + attrs.as_slice().buffer_len()];
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
fn test_pause_clears_volatile_state_keeps_wifi_phys_emited() {
    let mut worker = gen_worker();
    worker
        .emited
        .insert("enp1s0".to_string(), gen_event("enp1s0"));
    worker.wifi_phys_emited.insert("wlan0".to_string());
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
    // A wifi-phy already announced to the event worker is not forgotten
    // by a pause/resume cycle: only a real delete (or a fresh monitor
    // worker) should cause it to be announced as new again.
    assert!(worker.wifi_phys_emited.contains("wlan0"));
}

#[test]
fn test_notify_marks_new_wifi_phy_only_once() {
    let mut worker = gen_worker();
    worker.wifi_monitor_enabled = true;
    let (tx, _rx) = unbounded();
    worker.msg_to_commander = Some(tx);
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut event = InterfaceLinkEvent::new(
        "wlan0".to_string(),
        10,
        InterfaceType::WifiPhy,
        true,
        None,
    );
    rt.block_on(worker.notify(event.clone())).unwrap();
    assert!(worker.emited["wlan0"].is_new_wifi_phy);
    assert!(worker.wifi_phys_emited.contains("wlan0"));

    // A later event for the same phy is a normal link event.
    event.is_up = false;
    rt.block_on(worker.notify(event.clone())).unwrap();
    assert!(!worker.emited["wlan0"].is_new_wifi_phy);

    // A delete forgets the phy so a reappearance is announced again.
    event.is_delete = true;
    rt.block_on(worker.notify(event)).unwrap();
    assert!(!worker.wifi_phys_emited.contains("wlan0"));
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
fn test_pause_resume_nested_requires_matching_resume() {
    // `load_saved_state()` pauses the monitor for the whole boot pass while
    // every apply inside it pauses again. A nested resume must not start the
    // netlink socket (and emit a fresh link dump) before the outer pause is
    // released.
    let mut worker = gen_worker();
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(worker.process_cmd(NipartMonitorCmd::Pause))
        .unwrap();
    assert_eq!(worker.manual_pause_count, 1);

    rt.block_on(worker.process_cmd(NipartMonitorCmd::Pause))
        .unwrap();
    assert_eq!(worker.manual_pause_count, 2);

    rt.block_on(worker.process_cmd(NipartMonitorCmd::Resume))
        .unwrap();
    assert_eq!(worker.manual_pause_count, 1);
    assert!(worker.netlink_handle.is_none());

    rt.block_on(worker.process_cmd(NipartMonitorCmd::Resume))
        .unwrap();
    assert_eq!(worker.manual_pause_count, 0);
    assert!(worker.netlink_handle.is_none());
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
