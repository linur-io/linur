// SPDX-License-Identifier: GPL-3.0-only
//! The mount table (docs/perm.md §5).

use super::{Dentry, Inode};
use crate::drivers::BLOCK_DEVICE;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use easy_fs::EasyFileSystem;
use lazy_static::*;

/// One mounted filesystem: its device number and the root of its tree.
struct Mount {
    dev: u64,
    root: Arc<Dentry>,
}

/// The mounted filesystems, in mount order.
pub struct MountTable {
    mounts: Vec<Mount>,
}

impl MountTable {
    /// Mount the block device's filesystem at "/"; M2.3 has exactly one.
    fn new() -> Self {
        let efs = EasyFileSystem::open(BLOCK_DEVICE.clone());
        let root = Inode::new(Arc::new(EasyFileSystem::root_inode(&efs)));
        Self {
            mounts: vec![Mount {
                dev: 1,
                root: Arc::new(Dentry::new(String::from("/"), root)),
            }],
        }
    }

    /// The root dentry of the first mount.
    pub fn root(&self) -> Arc<Dentry> {
        Arc::clone(&self.mounts[0].root)
    }

    /// The device number of the filesystem every M2.3 inode belongs to.
    pub fn dev(&self) -> u64 {
        self.mounts[0].dev
    }
}

lazy_static! {
    static ref MOUNT_TABLE: MountTable = MountTable::new();
}

/// The filesystem root dentry.
pub fn root() -> Arc<Dentry> {
    MOUNT_TABLE.root()
}

/// The device number reported in `st_dev`.
pub fn dev() -> u64 {
    MOUNT_TABLE.dev()
}
