// SPDX-License-Identifier: Apache-2.0

use crate::{DnsResolver, MergedDnsResolver};

impl MergedDnsResolver {
    pub(crate) fn gen_diff(&self) -> DnsResolver {
        self.desired.clone().unwrap_or_default()
    }
}
