// SPDX-License-Identifier: Apache-2.0

mod apply;
mod plugin;
mod query;
mod scan;

pub(crate) struct NipartWpaConn;

use nipart::{NipartError, NipartPlugin};

use self::plugin::NipartPluginWifi;

fn init_logger() {
    let mut log_builder = env_logger::Builder::new();
    log_builder.filter(Some("nipart"), log::LevelFilter::Trace);
    log_builder.filter(Some("nipart_plugin"), log::LevelFilter::Trace);
    log_builder.filter(Some("nipart-plugin-wifi"), log::LevelFilter::Trace);
    log_builder.filter(Some("shuli"), log::LevelFilter::Trace);
    log_builder.init();
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), NipartError> {
    init_logger();
    NipartPluginWifi::run().await
}
