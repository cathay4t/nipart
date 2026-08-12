// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nmstate project(https://nmstate.io/) which
// is under license of Apache 2.0, authors of original file
// (rust/src/lib/ifaces/inter_ifaces_controller.rs) are:
//  * Gris Ge <fge@redhat.com>
//  * Wen Liang <liangwen12year@gmail.com>
//

use std::collections::{HashMap, HashSet};

use crate::{
    BondMode, ErrorKind, Interface, InterfaceIdentifier, InterfaceState,
    InterfaceType, MergedInterface, MergedInterfaces, NipartError,
    NipartInterface, OvsInterface,
};

fn is_port_overbook(
    port_to_ctrl: &mut HashMap<String, String>,
    port: &str,
    ctrl: &str,
) -> Result<(), NipartError> {
    if let Some(cur_ctrl) = port_to_ctrl.get(port) {
        let e = NipartError::new(
            ErrorKind::InvalidArgument,
            format!(
                "Port {port} is overbooked by two controller: {ctrl}, \
                 {cur_ctrl}"
            ),
        );
        log::error!("{e}");
        return Err(e);
    } else {
        port_to_ctrl.insert(port.to_string(), ctrl.to_string());
    }
    Ok(())
}

impl MergedInterface {
    /// Return two list, first is new port attached to specified interface,
    /// second is old port detached from specified interface.
    pub(crate) fn get_changed_ports(&self) -> Option<(Vec<&str>, Vec<&str>)> {
        let for_apply_iface = self.for_apply.as_ref()?;

        if for_apply_iface.is_absent() {
            {
                let ports = self.current.as_ref().and_then(|c| c.ports())?;
                return Some((Vec::new(), ports));
            }
        }

        let desired_port_names = match for_apply_iface.ports() {
            Some(p) => HashSet::from_iter(p.iter().cloned()),
            None => {
                // If current interface is in ignore state, even user did not
                // defining ports in desire, we should preserving existing port
                // lists
                {
                    let cur_iface = self.current.as_ref()?;
                    if cur_iface.is_ignore() {
                        cur_iface.ports().map(|ports| {
                            HashSet::<&str>::from_iter(ports.iter().cloned())
                        })?
                    } else {
                        return None;
                    }
                }
            }
        };

        let current_port_names = self
            .current
            .as_ref()
            .and_then(|cur_iface| {
                if cur_iface.is_ignore() {
                    None
                } else {
                    cur_iface.ports()
                }
            })
            .map(|ports| HashSet::<&str>::from_iter(ports.iter().cloned()))
            .unwrap_or_default();

        let chg_attached_ports: Vec<&str> = desired_port_names
            .difference(&current_port_names)
            .cloned()
            .collect();
        let chg_detached_ports: Vec<&str> = current_port_names
            .difference(&desired_port_names)
            .cloned()
            .collect();

        Some((chg_attached_ports, chg_detached_ports))
    }
}

impl MergedInterfaces {
    pub(crate) fn post_merge_sanitize_controller_and_port(
        &mut self,
    ) -> Result<(), NipartError> {
        self.post_merge_resolve_port_ref()?;
        self.handle_changed_ports()?;
        self.resolve_controller_type()?;
        self.drop_mac_identifier_bond_port_mac();
        self.check_overbook_ports()?;
        self.check_infiniband_as_ports()?;
        self.validate_controller_and_port_list_confliction()?;
        self.validate_can_have_ip()?;
        Ok(())
    }

    /// A port attached to a controller which does not allow IP on its ports
    /// (e.g. bond, linux bridge) must not have IP enabled in the desired
    /// state. VRF ports and OVS interfaces can hold IP.
    fn validate_can_have_ip(&self) -> Result<(), NipartError> {
        for merged_iface in
            self.iter().filter(|m| m.is_desired() && m.merged.is_up())
        {
            let Some(for_apply) = merged_iface.for_apply.as_ref() else {
                continue;
            };
            let base_iface = for_apply.base_iface();
            if !base_iface.can_have_ip()
                && (base_iface.ipv4.as_ref().and_then(|ipv4| ipv4.enabled)
                    == Some(true)
                    || base_iface.ipv6.as_ref().and_then(|ipv6| ipv6.enabled)
                        == Some(true))
            {
                if let Some(ctrl) = base_iface.controller.as_ref() {
                    return Err(NipartError::new(
                        ErrorKind::InvalidArgument,
                        format!(
                            "Interface {} cannot have IP enabled as it is \
                             attached to controller {ctrl} where IP is not \
                             allowed on its port",
                            base_iface.name.as_str()
                        ),
                    ));
                } else {
                    return Err(NipartError::new(
                        ErrorKind::InvalidArgument,
                        format!(
                            "Interface {} cannot have IP enabled",
                            base_iface.name.as_str()
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    fn post_merge_resolve_port_ref(&mut self) -> Result<(), NipartError> {
        let mut profile_to_kernel: HashMap<String, String> = HashMap::new();
        for merged_iface in self.iter() {
            let iface = &merged_iface.merged;
            if let Some(profile_name) = iface.base_iface().profile_name.as_ref()
                && !profile_name.is_empty()
                && !iface.kernel_iface_name().is_empty()
            {
                profile_to_kernel.insert(
                    profile_name.to_string(),
                    iface.kernel_iface_name().to_string(),
                );
            }
        }

        if profile_to_kernel.is_empty() {
            return Ok(());
        }

        let mut port_renames: Vec<(String, InterfaceType, String, String)> =
            Vec::new();
        for merged_iface in self.iter() {
            if !merged_iface.merged.is_controller() {
                continue;
            }
            if let Some(ports) = merged_iface.merged.ports() {
                for port_name in ports {
                    if let Some(kernel_name) = profile_to_kernel.get(port_name)
                    {
                        port_renames.push((
                            merged_iface.kernel_iface_name().to_string(),
                            merged_iface.iface_type().clone(),
                            port_name.to_string(),
                            kernel_name.clone(),
                        ));
                    }
                }
            }
        }

        for (ctrl_name, ctrl_type, org_port_name, new_port_name) in port_renames
        {
            let merged_iface = if ctrl_type.is_userspace() {
                self.user_ifaces
                    .get_mut(&(ctrl_name.clone(), ctrl_type.clone()))
            } else {
                self.kernel_ifaces.get_mut(&ctrl_name)
            };

            if let Some(merged_iface) = merged_iface {
                if let Some(for_apply) = merged_iface.for_apply.as_mut() {
                    for_apply.change_port_name(
                        &org_port_name,
                        new_port_name.clone(),
                    )?;
                }
                if let Some(for_verify) = merged_iface.for_verify.as_mut() {
                    for_verify.change_port_name(
                        &org_port_name,
                        new_port_name.clone(),
                    )?;
                }
                merged_iface
                    .merged
                    .change_port_name(&org_port_name, new_port_name)?;
            }
        }

        Ok(())
    }

    // Resolve controller_type for interfaces that have controller set
    // (e.g. from merge with current state) but controller_type is None.
    fn resolve_controller_type(&mut self) -> Result<(), NipartError> {
        let mut pending: Vec<(String, String)> = Vec::new();
        for merged_iface in self.iter() {
            if let Some(for_apply) = merged_iface.for_apply.as_ref()
                && let Some(ctrl_name) =
                    for_apply.base_iface().controller.as_ref()
                && !ctrl_name.is_empty()
                && for_apply.base_iface().controller_type.is_none()
            {
                pending.push((
                    merged_iface.kernel_iface_name().to_string(),
                    ctrl_name.to_string(),
                ));
            }
        }
        for (iface_name, ctrl_name) in pending {
            let ctrl_type = self
                .kernel_ifaces
                .get(&ctrl_name)
                .map(|c| c.merged.iface_type().clone())
                .or_else(|| {
                    self.user_ifaces.values().find_map(|c| {
                        if c.merged.name() == ctrl_name {
                            Some(c.merged.iface_type().clone())
                        } else {
                            None
                        }
                    })
                });
            if let Some(ctrl_type) = ctrl_type
                && let Some(merged_iface) =
                    self.kernel_ifaces.get_mut(&iface_name)
            {
                if let Some(for_apply) = merged_iface.for_apply.as_mut() {
                    for_apply.base_iface_mut().controller_type =
                        Some(ctrl_type.clone());
                }
                merged_iface.merged.base_iface_mut().controller_type =
                    Some(ctrl_type);
            }
        }
        Ok(())
    }

    /// The MAC address of a bond port is controlled by the bond kernel
    /// driver (e.g. in active-backup mode the kernel assigns the bond's MAC
    /// to every slave), so for `identifier: mac-address` configs the MAC is
    /// only an identifier used to locate the interface — it cannot be applied
    /// or verified while the interface is enslaved by a bond.
    /// `MergedInterface::apply_ctrller_change()` clears it from `for_verify`
    /// for ports newly attached in this apply; this pass additionally covers
    /// the steady-state case where the interface is already a bond port and
    /// no controller change is triggered (and clears `for_apply` as well).
    fn drop_mac_identifier_bond_port_mac(&mut self) {
        // Collect the kernel names first so we do not hold a mutable borrow
        // while resolving the controller type.
        let mut bond_port_names: Vec<String> = Vec::new();
        for merged_iface in self.iter() {
            let Some(for_apply) = merged_iface.for_apply.as_ref() else {
                continue;
            };
            if for_apply.base_iface().identifier
                != Some(InterfaceIdentifier::MacAddress)
            {
                continue;
            }
            let ctrl_name = merged_iface
                .merged
                .base_iface()
                .controller
                .as_deref()
                .unwrap_or_default();
            if ctrl_name.is_empty() {
                continue;
            }
            let ctrl_type = merged_iface
                .merged
                .base_iface()
                .controller_type
                .clone()
                .or_else(|| {
                    self.kernel_ifaces
                        .get(ctrl_name)
                        .map(|c| c.merged.iface_type().clone())
                })
                .or_else(|| {
                    self.user_ifaces.values().find_map(|c| {
                        (c.merged.name() == ctrl_name)
                            .then(|| c.merged.iface_type().clone())
                    })
                });
            if ctrl_type == Some(InterfaceType::Bond) {
                bond_port_names
                    .push(merged_iface.kernel_iface_name().to_string());
            }
        }
        for name in bond_port_names {
            if let Some(merged_iface) = self.kernel_ifaces.get_mut(&name) {
                if let Some(for_apply) = merged_iface.for_apply.as_mut() {
                    for_apply.base_iface_mut().mac_address = None;
                }
                if let Some(for_verify) = merged_iface.for_verify.as_mut() {
                    for_verify.base_iface_mut().mac_address = None;
                }
            }
        }
    }

    // Check whether user defined both controller property and port list of
    // controller interface, examples of invalid desire state:
    //  * eth1 has controller: br1, but br1 has no eth1 in port list
    //  * eth2 has controller: br1, but br2 has eth2 in port list
    //  * eth1 has controller: Some("") (detach), but br1 has eth1 in port list
    fn validate_controller_and_port_list_confliction(
        &self,
    ) -> Result<(), NipartError> {
        self.validate_controller_not_in_port_list()?;
        self.validate_controller_in_other_port_list()?;
        Ok(())
    }

    fn validate_controller_not_in_port_list(&self) -> Result<(), NipartError> {
        for merged_iface in self.kernel_ifaces.values() {
            if merged_iface.desired.is_none() || !merged_iface.merged.is_up() {
                continue;
            }

            if let Some(des_ctrl_name) = merged_iface
                .desired
                .as_ref()
                .and_then(|i| i.base_iface().controller.as_ref())
            {
                // Detaching from current controller
                if des_ctrl_name.is_empty() {
                    continue;
                }

                if let Some(ctrl_iface) = self
                    .user_ifaces
                    .get(&(des_ctrl_name.to_string(), InterfaceType::OvsBridge))
                    .or_else(|| self.kernel_ifaces.get(des_ctrl_name))
                {
                    // controller iface not mentioned in desire state
                    if !ctrl_iface.is_desired() {
                        continue;
                    }
                    if let Some(ports) = ctrl_iface
                        .desired
                        .as_ref()
                        .and_then(|desire| desire.ports())
                        && !ports
                            .contains(&merged_iface.merged.kernel_iface_name())
                    {
                        return Err(NipartError::new(
                            ErrorKind::InvalidArgument,
                            format!(
                                "Interface {} has controller {} but not \
                                 listed in port list of controller interface",
                                merged_iface.merged.name(),
                                des_ctrl_name,
                            ),
                        ));
                    }
                } else {
                    return Err(NipartError::new(
                        ErrorKind::InvalidArgument,
                        format!(
                            "Interface {} desired controller {des_ctrl_name} \
                             not found",
                            merged_iface.merged.name()
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    fn validate_controller_in_other_port_list(
        &self,
    ) -> Result<(), NipartError> {
        let mut port_to_ctrl = HashMap::new();
        for iface in self.iter().filter(|i| i.is_desired() && i.merged.is_up())
        {
            if let Some(port_names) = iface.merged.ports() {
                for port_name in port_names {
                    port_to_ctrl
                        .insert(port_name, iface.merged.kernel_iface_name());
                }
            }
        }
        for iface in self
            .kernel_ifaces
            .values()
            .filter(|i| i.is_desired() && i.merged.is_up())
        {
            let des_ctrl_name = if let Some(n) = iface
                .desired
                .as_ref()
                .and_then(|i| i.base_iface().controller.as_ref())
            {
                n
            } else {
                continue;
            };
            if let Some(ctrl_name) =
                port_to_ctrl.get(iface.merged.kernel_iface_name())
                && des_ctrl_name != ctrl_name
            {
                if des_ctrl_name.is_empty() {
                    return Err(NipartError::new(
                        ErrorKind::InvalidArgument,
                        format!(
                            "Interface {} desired to detach controller via \
                             controller property set to '', but still been \
                             listed as port of controller {} ",
                            iface.merged.name(),
                            ctrl_name
                        ),
                    ));
                } else {
                    return Err(NipartError::new(
                        ErrorKind::InvalidArgument,
                        format!(
                            "Interface {} has controller property set to {}, \
                             but been listed as port of controller {} ",
                            iface.merged.name(),
                            des_ctrl_name,
                            ctrl_name
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    fn handle_changed_ports(&mut self) -> Result<(), NipartError> {
        let mut pending_changes: HashMap<
            // kernel_iface_name
            String,
            // (controller_naem, controller_type, interface_state)
            (String, InterfaceType, InterfaceState),
        > = HashMap::new();
        for merged_iface in self.iter().filter(|m| m.is_changed()) {
            if !merged_iface.is_desired()
                || !merged_iface.merged.is_controller()
            {
                continue;
            }
            let Some(for_apply) = merged_iface.for_apply.as_ref() else {
                continue;
            };

            if let Some((attached_ports, detached_ports)) =
                merged_iface.get_changed_ports()
            {
                for port_name in attached_ports {
                    pending_changes.insert(
                        port_name.to_string(),
                        (
                            for_apply.kernel_iface_name().to_string(),
                            for_apply.iface_type().clone(),
                            for_apply.base_iface().state,
                        ),
                    );
                }
                for port_name in detached_ports {
                    // Port might move from one controller to another, if there
                    // is already a pending action for this
                    // port, we don't override it.
                    pending_changes
                        .entry(port_name.to_string())
                        .or_insert_with(|| {
                            (
                                String::new(),
                                InterfaceType::Unknown(String::new()),
                                for_apply.base_iface().state,
                            )
                        });
                }
            }
        }

        // We are assuming all ports are kernel interfaces. It is true
        // for now even for OVS bridge, but worth noting here.
        for (iface_name, (ctrl_name, ctrl_type, ctrl_state)) in
            pending_changes.drain()
        {
            if let Some(merged_iface) = self.kernel_ifaces.get_mut(&iface_name)
            {
                if !merged_iface.is_changed() {
                    self.insert_order.push((
                        merged_iface.merged.kernel_iface_name().to_string(),
                        merged_iface.merged.iface_type().clone(),
                    ));
                }
                merged_iface
                    .apply_ctrller_change(ctrl_name, ctrl_type, ctrl_state)?;
            } else if ctrl_type == InterfaceType::OvsBridge {
                // OVS internal interface could be created by its controller OVS
                // Bridge
                log::info!(
                    "Creating new OVS internal interface {iface_name} to edit \
                     as its controller {ctrl_name} required so",
                );
                self.kernel_ifaces.insert(
                    iface_name.to_string(),
                    MergedInterface::new(
                        Some(Interface::OvsInterface(Box::new(
                            OvsInterface::new_with_name_and_ctrl(
                                &iface_name,
                                &ctrl_name,
                            ),
                        ))),
                        None,
                        None,
                    )?,
                );
                self.insert_order.push((
                    iface_name.to_string(),
                    InterfaceType::OvsInterface,
                ));
            } else if !ctrl_name.is_empty() {
                return Err(NipartError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "Controller interface {ctrl_name} is holding port \
                         {iface_name} which does not exists"
                    ),
                ));
            }
        }
        Ok(())
    }

    fn check_overbook_ports(&self) -> Result<(), NipartError> {
        let mut port_to_ctrl: HashMap<String, String> = HashMap::new();
        for iface in self.iter().filter(|i| {
            i.merged.is_controller() && i.merged.is_up() && i.is_desired()
        }) {
            let ports = if let Some(p) = iface.merged.ports() {
                p
            } else {
                continue;
            };

            for port in ports {
                is_port_overbook(
                    &mut port_to_ctrl,
                    port,
                    iface.merged.kernel_iface_name(),
                )?;
            }
        }

        Ok(())
    }

    // Infiniband over IP can only be port of active_backup bond as it is a
    // layer 3 interface like tun.
    fn check_infiniband_as_ports(&self) -> Result<(), NipartError> {
        let ib_iface_names: HashSet<&str> = self
            .kernel_ifaces
            .values()
            .filter(|iface| {
                iface.merged.iface_type() == &InterfaceType::InfiniBand
            })
            .map(|iface| iface.merged.kernel_iface_name())
            .collect();

        for iface in self
            .kernel_ifaces
            .values()
            .filter(|i| i.is_desired() && i.merged.is_controller())
            .map(|i| &i.merged)
        {
            if let Some(ports) = iface.ports() {
                let ports = HashSet::from_iter(ports.iter().cloned());
                if !ib_iface_names.is_disjoint(&ports) {
                    if let Interface::Bond(iface) = iface
                        && iface.mode() == Some(BondMode::ActiveBackup)
                    {
                        continue;
                    }
                    let e = NipartError::new(
                        ErrorKind::InvalidArgument,
                        format!(
                            "InfiniBand interface {:?} cannot use as port of \
                             {}. Only active-backup bond allowed.",
                            ib_iface_names.intersection(&ports),
                            iface.name()
                        ),
                    );
                    log::error!("{e}");
                    return Err(e);
                }
            }
        }
        Ok(())
    }
}
