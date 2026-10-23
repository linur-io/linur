// SPDX-License-Identifier: GPL-3.0-only
//! Memory syscalls: brk, mmap and munmap.

use crate::config::PAGE_SIZE;
use crate::errno::{EINVAL, ENOMEM};
use crate::mm::{MapPermission, VirtAddr};
use crate::task::current_process;

/// Highest user virtual address SV39 can name; a larger one would be masked.
const USER_ADDR_TOP: usize = 1 << 38;

/// Set the program break: 0 returns the current break, otherwise the heap grows
/// or shrinks to addr. ENOMEM when addr is below the heap bottom or would
/// overlap another mapping, in which case nothing changes.
pub fn sys_brk(addr: usize) -> isize {
    if addr > USER_ADDR_TOP {
        return ENOMEM;
    }
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    match inner.memory_set.brk(addr) {
        Some(new_break) => new_break as isize,
        None => ENOMEM,
    }
}

/// Map len bytes of anonymous memory at start. prot is a non-empty subset of
/// PROT_READ (1), PROT_WRITE (2) and PROT_EXEC (4), and the address is fixed.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    if !start.is_multiple_of(PAGE_SIZE) || prot & !0x7 != 0 || prot & 0x7 == 0 {
        return EINVAL;
    }
    if len == 0 {
        return 0;
    }
    let Some(end) = start.checked_add(len) else {
        return EINVAL;
    };
    if end > USER_ADDR_TOP {
        return EINVAL;
    }
    let mut permission = MapPermission::U;
    if prot & 0x1 != 0 {
        permission |= MapPermission::R;
    }
    if prot & 0x2 != 0 {
        permission |= MapPermission::W;
    }
    if prot & 0x4 != 0 {
        permission |= MapPermission::X;
    }
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if inner
        .memory_set
        .mmap(VirtAddr::from(start), VirtAddr::from(end), permission)
    {
        0
    } else {
        EINVAL
    }
}

/// Unmap len bytes at start. EINVAL, with nothing unmapped, when any page in
/// the range is not mapped.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    if !start.is_multiple_of(PAGE_SIZE) {
        return EINVAL;
    }
    if len == 0 {
        return 0;
    }
    let Some(end) = start.checked_add(len) else {
        return EINVAL;
    };
    if end > USER_ADDR_TOP {
        return EINVAL;
    }
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if inner
        .memory_set
        .munmap(VirtAddr::from(start), VirtAddr::from(end))
    {
        0
    } else {
        EINVAL
    }
}
