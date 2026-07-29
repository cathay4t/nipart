// SPDX-License-Identifier: Apache-2.0

// This file is based on the work of nmstate project(https://nmstate.io/) which
// is under license of Apache 2.0, authors of original file are:
//  * Gris Ge <fge@redhat.com>
//  * Fernando Fernandez Mancera <ffmancera@riseup.net>
//  * Ales Musil <amusil@redhat.com>
//  * Jan Vaclav <jvaclav@redhat.com>

use serde::{Deserialize, Serialize};

use crate::{
    BaseInterface, ErrorKind, InterfaceType, JsonDisplay, NipartError,
    NipartInterface,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonDisplay)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[non_exhaustive]
/// OpenvSwitch Bridge
pub struct OvsBridgeInterface {
    #[serde(flatten)]
    pub base: BaseInterface,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bridge: Option<OvsBridgeConfig>,
}

impl OvsBridgeInterface {
    pub(crate) fn change_port_name(
        &mut self,
        org_port_name: &str,
        new_port_name: String,
    ) -> Result<(), NipartError> {
        if let Some(br_conf) = self.bridge.as_mut()
            && let Some(ports) = br_conf.ports.as_mut()
        {
            for port in ports {
                if port.name == org_port_name {
                    port.name = new_port_name;
                    return Ok(());
                }
            }
        }
        Err(NipartError::new(
            ErrorKind::InvalidArgument,
            format!(
                "Port {} not found in OVS bridge {}",
                org_port_name,
                self.base.name.as_str()
            ),
        ))
    }

    pub fn new(base: BaseInterface, bridge: Option<OvsBridgeConfig>) -> Self {
        Self { base, bridge }
    }
}

impl Default for OvsBridgeInterface {
    fn default() -> Self {
        Self {
            base: BaseInterface {
                iface_type: InterfaceType::OvsBridge,
                ..Default::default()
            },
            bridge: None,
        }
    }
}

impl NipartInterface for OvsBridgeInterface {
    fn base_iface(&self) -> &BaseInterface {
        &self.base
    }

    fn base_iface_mut(&mut self) -> &mut BaseInterface {
        &mut self.base
    }

    fn is_virtual(&self) -> bool {
        true
    }

    fn hide_secrets(&mut self) {}

    fn sanitize(
        &self,
        _current: Option<&Self>,
        _for_save: &mut Self,
        _for_apply: &mut Self,
        _for_verify: &mut Self,
        _merged: &mut Self,
    ) -> Result<(), NipartError> {
        Ok(())
    }

    fn include_diff_context(&mut self, _desired: &Self, _current: &Self) {
        // TODO(Gris Ge): Include full port config if any changed
    }

    fn include_revert_context(&mut self, _desired: &Self, _pre_apply: &Self) {
        // TODO(Gris Ge): Include full port config if any changed
    }

    fn ports(&self) -> Option<Vec<&str>> {
        if let Some(br_conf) = &self.bridge
            && let Some(port_confs) = &br_conf.ports
        {
            let mut port_names = Vec::new();
            for port_conf in port_confs {
                port_names.push(port_conf.name.as_str());
            }
            return Some(port_names);
        }
        None
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonDisplay,
)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[non_exhaustive]
pub struct OvsBridgeConfig {
    /// Serialize to 'port'. Deserialize from `port` or `ports`.
    pub ports: Option<Vec<OvsBridgePortConfig>>,
}

#[derive(
    Debug, Clone, PartialEq, Default, Eq, Serialize, Deserialize, JsonDisplay,
)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[non_exhaustive]
pub struct OvsBridgePortConfig {
    pub name: String,
}
