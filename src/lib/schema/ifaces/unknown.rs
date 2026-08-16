// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::{BaseInterface, JsonDisplay, NipartError, NipartInterface};

#[derive(
    Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonDisplay,
)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
/// Holder for interface with unknown interface type defined.
/// During apply action, nipart can resolve unknown interface to first
/// found interface type.
pub struct UnknownInterface {
    #[serde(flatten)]
    pub base: BaseInterface,
}

impl UnknownInterface {
    pub fn new(base: BaseInterface) -> Self {
        Self {
            base,
            ..Default::default()
        }
    }
}

impl NipartInterface for UnknownInterface {
    fn base_iface(&self) -> &BaseInterface {
        &self.base
    }

    fn base_iface_mut(&mut self) -> &mut BaseInterface {
        &mut self.base
    }

    /// Not sure is physical or kernel virtual interface, treat as virtual
    /// always.
    fn is_virtual(&self) -> bool {
        true
    }

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
}

#[cfg(test)]
mod tests {
    use super::UnknownInterface;
    use crate::{
        BaseInterface, InterfaceLinkState, InterfaceState, InterfaceType,
    };

    #[test]
    fn test_deserialize_preserves_query_only_base_fields() {
        let base = BaseInterface {
            name: "vnet0".to_string(),
            iface_type: InterfaceType::Tun,
            state: InterfaceState::Ignore,
            iface_index: Some(6),
            mtu: Some(1500),
            mac_address: Some("FE:54:00:D9:4F:3E".to_string()),
            controller: Some("virbr0".to_string()),
            link_state: Some(InterfaceLinkState::Unknown),
            ..Default::default()
        };
        let iface = UnknownInterface::new(base);
        let value = serde_json::to_value(&iface).unwrap();

        let roundtrip: UnknownInterface =
            serde_json::from_value(value).unwrap();
        assert_eq!(roundtrip.base.iface_index, Some(6));
        assert_eq!(roundtrip.base.mtu, Some(1500));
        assert_eq!(roundtrip.base.controller.as_deref(), Some("virbr0"));
        assert_eq!(
            roundtrip.base.link_state,
            Some(InterfaceLinkState::Unknown)
        );
    }
}
