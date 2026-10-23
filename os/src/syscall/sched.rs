// SPDX-License-Identifier: GPL-3.0-only
//! Scheduling syscalls.

use crate::config::MIN_PRIORITY;
use crate::errno::EINVAL;
use crate::task::current_task;

/// Set the calling task's scheduling weight and return it; a weight below
/// `MIN_PRIORITY` is `EINVAL`.
pub fn sys_set_priority(priority: isize) -> isize {
    if priority < MIN_PRIORITY {
        return EINVAL;
    }
    current_task().unwrap().inner_exclusive_access().priority = priority;
    priority
}
