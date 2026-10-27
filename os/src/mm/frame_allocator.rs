// SPDX-License-Identifier: GPL-3.0-only
use super::{PhysAddr, PhysPageNum};
use crate::config::{MEMORY_END, MEMORY_START, PAGE_SIZE};
use crate::sync::SpinMutex;
use core::fmt::{self, Debug, Formatter};
use core::ptr::addr_of_mut;

/// One byte per frame of board RAM, indexed by `ppn - first`; the C module walks it.
static mut FRAME_MAP: [u8; (MEMORY_END - MEMORY_START) / PAGE_SIZE] =
    [0; (MEMORY_END - MEMORY_START) / PAGE_SIZE];

/// The C module has no lock of its own, so every call takes this one.
static FRAME_LOCK: SpinMutex<()> = SpinMutex::new(());

/// The ABI of `alloc/include/alloc/frame.h` (docs/alloc.md §3).
mod c {
    unsafe extern "C" {
        pub fn frame_init(map: *mut u8, first: usize, end: usize);
        pub fn frame_alloc() -> isize;
        pub fn frame_free(ppn: usize) -> i32;
    }
}

pub struct FrameTracker {
    pub ppn: PhysPageNum,
}

impl FrameTracker {
    pub fn new(ppn: PhysPageNum) -> Self {
        // page cleaning
        let bytes_array = ppn.get_bytes_array();
        for i in bytes_array {
            *i = 0;
        }
        Self { ppn }
    }
}

impl Debug for FrameTracker {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("FrameTracker:PPN={:#x}", self.ppn.0))
    }
}

impl Drop for FrameTracker {
    fn drop(&mut self) {
        frame_dealloc(self.ppn);
    }
}

pub fn init_frame_allocator() {
    unsafe extern "C" {
        safe fn ekernel();
    }
    let first = PhysAddr::from(linker_symbol_addr!(ekernel)).ceil().0;
    let end = PhysAddr::from(MEMORY_END).floor().0;
    let _lock = FRAME_LOCK.lock();
    unsafe { c::frame_init(addr_of_mut!(FRAME_MAP) as *mut u8, first, end) };
    println!("last {} Physical Frames.", end - first);
}

pub fn frame_alloc() -> Option<FrameTracker> {
    let _lock = FRAME_LOCK.lock();
    let ppn = unsafe { c::frame_alloc() };
    (ppn >= 0).then(|| FrameTracker::new(PhysPageNum(ppn as usize)))
}

pub fn frame_dealloc(ppn: PhysPageNum) {
    let _lock = FRAME_LOCK.lock();
    if unsafe { c::frame_free(ppn.0) } != 0 {
        panic!("Frame ppn={:#x} has not been allocated!", ppn.0);
    }
}

#[allow(unused)]
pub fn frame_allocator_test() {
    let mut v: alloc::vec::Vec<FrameTracker> = alloc::vec::Vec::new();
    for i in 0..5 {
        let frame = frame_alloc().unwrap();
        println!("{:?}", frame);
        v.push(frame);
    }
    v.clear();
    for i in 0..5 {
        let frame = frame_alloc().unwrap();
        println!("{:?}", frame);
        v.push(frame);
    }
    drop(v);
    println!("frame_allocator_test passed!");
}
