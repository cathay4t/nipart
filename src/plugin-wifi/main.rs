// SPDX-License-Identifier: Apache-2.0

#[macro_use]
mod dbus_macros;

mod apply;
mod bss;
mod dbus;
mod interface;
mod network;
mod plugin;
mod query;
mod scan;

#[derive(Debug)]
pub(crate) struct NipartWpaConn;

use nipart::{NipartError, NipartPlugin};

use self::plugin::NipartPluginWifi;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), NipartError> {
    NipartPluginWifi::run().await
}
