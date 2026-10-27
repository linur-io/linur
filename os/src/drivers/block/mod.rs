// SPDX-License-Identifier: GPL-3.0-only
mod virtio_blk;

pub use virtio_blk::VirtIOBlock;

use crate::board::BlockDeviceImpl;
use alloc::sync::Arc;
use easy_fs::BlockDevice;
use lazy_static::*;

lazy_static! {
    /// The one block device every filesystem call goes through (docs/block.md §5).
    pub static ref BLOCK_DEVICE: Arc<dyn BlockDevice> = Arc::new(BlockDeviceImpl::new());
}
