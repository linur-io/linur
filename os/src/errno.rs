// SPDX-License-Identifier: GPL-3.0-only
//! Linux error numbers.
//!
//! A syscall returns the negated value: `-ENOENT` is `-2`. This is the set the
//! current calls can produce; a new call adds its numbers here.

/// No such file or directory.
pub const ENOENT: isize = -2;
/// No such process or thread.
pub const ESRCH: isize = -3;
/// Argument list too long.
pub const E2BIG: isize = -7;
/// Executable format error.
pub const ENOEXEC: isize = -8;
/// Bad file descriptor.
pub const EBADF: isize = -9;
/// No child process.
pub const ECHILD: isize = -10;
/// Child or thread has not exited yet.
pub const EAGAIN: isize = -11;
/// Bad address: the user pointer range is not mapped, not user-accessible, or
/// not writable where the kernel writes.
pub const EFAULT: isize = -14;
/// Invalid argument to a known syscall.
pub const EINVAL: isize = -22;
/// The deadlock detector refuses the request.
pub const EDEADLK: isize = -35;
/// A name longer than one directory entry.
pub const ENAMETOOLONG: isize = -36;
/// Unknown syscall id.
pub const ENOSYS: isize = -38;
