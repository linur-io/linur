// SPDX-License-Identifier: GPL-3.0-only
//! Linux error numbers.
//!
//! A syscall returns the negated value. M1.3a introduces the three numbers the
//! user-memory layer and dispatch need; M1.3b converts the whole ABI to errno.

/// Bad address: the user pointer range is not mapped, not user-accessible, or
/// not writable where the kernel writes.
pub const EFAULT: isize = -14;

/// Invalid argument to a known syscall.
pub const EINVAL: isize = -22;

/// Unknown syscall id.
pub const ENOSYS: isize = -38;
