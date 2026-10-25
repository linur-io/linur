// SPDX-License-Identifier: GPL-3.0-only
//! POSIX-style errno.
//!
//! The kernel returns `-errno`. The wrappers in [`crate::syscall`] turn a
//! negative result into `-1` and record the number here, so a program checks
//! the return value and asks [`errno`] why. One global until the real
//! per-thread libc (M3.3).

use core::sync::atomic::{AtomicI32, Ordering};

/// The errno of the last failing wrapper call.
static ERRNO: AtomicI32 = AtomicI32::new(0);

/// The errno of this thread's last failing wrapper call.
pub fn errno() -> i32 {
    ERRNO.load(Ordering::Relaxed)
}

/// Record the errno of a failing raw syscall, which returned `-errno`.
pub(crate) fn set(errno: isize) {
    ERRNO.store(errno as i32, Ordering::Relaxed);
}

/// No such file or directory.
pub const ENOENT: i32 = 2;
/// No such process or thread.
pub const ESRCH: i32 = 3;
/// Argument list too long.
pub const E2BIG: i32 = 7;
/// Executable format error.
pub const ENOEXEC: i32 = 8;
/// Bad file descriptor.
pub const EBADF: i32 = 9;
/// No child process.
pub const ECHILD: i32 = 10;
/// Temporarily unavailable; the wait wrappers retry on this.
pub const EAGAIN: i32 = 11;
/// Out of memory: the request would exceed or overlap the user address space.
pub const ENOMEM: i32 = 12;
/// Bad address.
pub const EFAULT: i32 = 14;
/// File exists.
pub const EEXIST: i32 = 17;
/// A path component is not a directory.
pub const ENOTDIR: i32 = 20;
/// Is a directory.
pub const EISDIR: i32 = 21;
/// Invalid argument.
pub const EINVAL: i32 = 22;
/// Illegal seek: the file is a pipe or a terminal.
pub const ESPIPE: i32 = 29;
/// Numerical result out of range, such as a too-small `getcwd` buffer.
pub const ERANGE: i32 = 34;
/// Operation would deadlock.
pub const EDEADLK: i32 = 35;
/// Name too long for one directory entry.
pub const ENAMETOOLONG: i32 = 36;
/// Unknown syscall.
pub const ENOSYS: i32 = 38;
/// Directory not empty.
pub const ENOTEMPTY: i32 = 39;
