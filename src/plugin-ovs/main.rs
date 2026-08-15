// SPDX-License-Identifier: Apache-2.0

mod db;
mod json_rpc;
mod method;
mod operation;
mod plugin;
mod query;

use nipart::{NipartError, NipartPlugin};

use self::plugin::NipartPluginOvs;

fn init_logger() {
    let mut log_builder = env_logger::Builder::new();
    log_builder.filter(Some("nipart"), log::LevelFilter::Debug);
    log_builder.filter(Some("nipart_plugin"), log::LevelFilter::Debug);
    log_builder.filter(Some("nipart-plugin-ovs"), log::LevelFilter::Debug);
    log_builder.init();
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), NipartError> {
    init_logger();
    NipartPluginOvs::run().await
}
