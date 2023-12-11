// SPDX-License-Identifier: MIT

use serde::{Deserialize, Serialize};

use crate::{NipartQueryStateOption, NipartPluginInfo};

/// Events should be supported by all plugins.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum NipartPluginCommonEvent {
    QueryPluginInfo,
    QueryPluginInfoReply(NipartPluginInfo),
    Quit,
}