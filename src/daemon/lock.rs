// SPDX-License-Identifier: Apache-2.0

use std::{
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicI32, Ordering},
};

use tokio::sync::{Mutex, MutexGuard};

static CUR_LOCKER_PID: AtomicI32 = AtomicI32::new(0);
static LOCK: Mutex<()> = Mutex::const_new(());

pub(crate) struct NipartLockGuard(MutexGuard<'static, ()>);

impl Deref for NipartLockGuard {
    type Target = ();

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for NipartLockGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for NipartLockGuard {
    fn drop(&mut self) {
        CUR_LOCKER_PID.store(0, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NipartLockManager;

impl NipartLockManager {
    pub(crate) fn cur_locker_pid() -> Option<i32> {
        let cur_pid = CUR_LOCKER_PID.load(Ordering::Relaxed);
        if cur_pid == 0 { None } else { Some(cur_pid) }
    }

    pub(crate) async fn lock(pid: i32) -> NipartLockGuard {
        let ret = LOCK.lock().await;
        CUR_LOCKER_PID.store(pid, Ordering::Relaxed);
        NipartLockGuard(ret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cur_locker_pid_cleared_on_release() {
        assert_eq!(NipartLockManager::cur_locker_pid(), None);
        {
            let _guard = NipartLockManager::lock(12345).await;
            assert_eq!(NipartLockManager::cur_locker_pid(), Some(12345));
        }
        assert_eq!(NipartLockManager::cur_locker_pid(), None);
    }
}
