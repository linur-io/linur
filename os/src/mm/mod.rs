// SPDX-License-Identifier: GPL-3.0-only
mod address;
mod frame_allocator;
mod heap_allocator;
mod memory_set;
mod page_table;
mod user;

use address::VPNRange;
pub use address::{PhysAddr, PhysPageNum, StepByOne, VirtAddr, VirtPageNum};
pub use frame_allocator::{FrameTracker, frame_alloc, frame_dealloc};
pub use memory_set::remap_test;
pub use memory_set::{KERNEL_SPACE, MapPermission, MemorySet, kernel_token};
use page_table::PTEFlags;
pub use page_table::{PageTable, PageTableEntry};
pub use user::{
    UserBuffer, copy_from_user, copy_to_user, translated_byte_buffer, translated_byte_buffer_mut,
    translated_str,
};

pub fn init() {
    heap_allocator::init_heap();
    frame_allocator::init_frame_allocator();
    KERNEL_SPACE.exclusive_access().activate();
}
