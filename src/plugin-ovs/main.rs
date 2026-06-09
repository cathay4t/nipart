// SPDX-License-Identifier: Apache-2.0

mod db;
mod json_rpc;
mod method;
mod operation;
mod plugin;
mod query;

use nipart::{NipartError, NipartPlugin};

use self::plugin::NipartPluginOvs;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), NipartError> {
    NipartPluginOvs::run().await
}
