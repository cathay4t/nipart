// SPDX-License-Identifier: Apache-2.0

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
