// SPDX-License-Identifier: GPL-3.0-only
//! Credentials and the permission predicate (docs/perm.md §2, §6).

use super::Inode;
use crate::errno::EACCES;
use crate::task::current_process;

/// Search permission on a directory.
pub const MAY_EXEC: u8 = 1;
/// Write permission.
pub const MAY_WRITE: u8 = 2;
/// Read permission.
pub const MAY_READ: u8 = 4;

/// The identity a permission check runs as.
#[derive(Clone, Copy)]
pub struct Cred {
    /// User id.
    pub uid: u32,
    /// Group id.
    pub gid: u32,
}

impl Cred {
    /// The superuser, for boot-time path resolution.
    pub const ROOT: Cred = Cred { uid: 0, gid: 0 };

    /// The calling process's identity.
    pub fn current() -> Self {
        let process = current_process();
        let inner = process.inner_exclusive_access();
        Self {
            uid: inner.uid,
            gid: inner.gid,
        }
    }
}

/// Whether `inode` grants `mask` to `cred`. The owner bits are read first, then
/// the group bits, then the other bits, and every requested bit must be granted;
/// the superuser bypasses all of it, as 0.11's `suser()` does.
pub fn permission(inode: &Inode, mask: u8, cred: Cred) -> Result<(), isize> {
    if cred.uid == 0 {
        return Ok(());
    }
    let mode = inode.mode();
    let bits = if cred.uid == inode.uid() {
        (mode >> 6) & 0o7
    } else if cred.gid == inode.gid() {
        (mode >> 3) & 0o7
    } else {
        mode & 0o7
    };
    if bits & mask as u32 == mask as u32 {
        Ok(())
    } else {
        Err(EACCES)
    }
}
