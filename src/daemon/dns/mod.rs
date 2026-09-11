// SPDX-License-Identifier: Apache-2.0

//! DNS cache server task.
//!
//! The cache implementation is ported from the `mudz` project (same author,
//! Apache-2.0).  The daemon starts/updates/stops the server through
//! [`NipartDnsManager`] instead of running it as a standalone daemon.

mod cache;
mod config;
mod doh;
mod group;
mod host;
mod listener;
mod manager;
mod resolver;
mod retry;
mod server;
mod worker;

pub(crate) use self::{
    config::NipartDnsServerConfig,
    manager::NipartDnsManager,
    server::DnsCacheServer,
    worker::{NipartDnsCmd, NipartDnsReply, NipartDnsWorker},
};
