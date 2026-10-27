// SPDX-License-Identifier: GPL-3.0-only
//! A first-fit free-list allocator: the list lives inside the free memory itself.

#![no_std]
#![deny(missing_docs)]

use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use core::hint::spin_loop;
use core::ptr;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};

/// A block header, kept in the first bytes of the block it describes.
struct Block {
    /// Whole block size in bytes, header included.
    size: usize,
    /// Next free block, address-ordered; null at the end of the list.
    next: *mut Block,
}

/// Header size, which is also the room reserved before every payload.
const HEADER: usize = core::mem::size_of::<Block>();
/// Payloads are 16-byte aligned unless the caller asks for more.
const MIN_ALIGN: usize = 16;
/// A free block smaller than this is not worth splitting off.
const MIN_BLOCK: usize = HEADER + MIN_ALIGN;

/// A heap that serialises allocation behind a spinlock.
pub struct LockedHeap {
    /// Free list head, ascending by address; null when the heap is empty.
    head: Cell<*mut Block>,
    /// Held across each alloc and dealloc.
    lock: AtomicBool,
}

/// The free list is only touched with the spinlock held.
unsafe impl Sync for LockedHeap {}

impl LockedHeap {
    /// A heap with no memory: call init before allocating.
    pub const fn empty() -> Self {
        Self {
            head: Cell::new(ptr::null_mut()),
            lock: AtomicBool::new(false),
        }
    }

    /// Adopt the `size` bytes at `start` as free memory.
    ///
    /// # Safety
    ///
    /// `[start, start + size)` must be free memory nobody else touches.
    pub unsafe fn init(&self, start: usize, size: usize) {
        self.acquire();
        let end = start + size;
        let start = align_up(start, MIN_ALIGN);
        // A tail too small for a header plus a minimum payload is left out.
        let size = end.saturating_sub(start) & !(MIN_ALIGN - 1);
        self.head.set(if size < MIN_BLOCK {
            ptr::null_mut()
        } else {
            unsafe { write_block(start, size, ptr::null_mut()) }
        });
        self.release();
    }

    /// Take the spinlock: uniprocessor semantics, no interrupt masking.
    fn acquire(&self) {
        while self
            .lock
            .compare_exchange_weak(false, true, Acquire, Relaxed)
            .is_err()
        {
            spin_loop();
        }
    }

    /// Release the spinlock.
    fn release(&self) {
        self.lock.store(false, Release);
    }

    /// Link `node` after `prev` in the address-ordered list; null `prev` is the head.
    fn relink(&self, prev: *mut Block, node: *mut Block) {
        if prev.is_null() {
            self.head.set(node);
        } else {
            unsafe { (*prev).next = node };
        }
    }
}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(MIN_ALIGN);
        let size = align_up(layout.size(), MIN_ALIGN);
        self.acquire();
        let mut prev = ptr::null_mut();
        let mut block = self.head.get();
        while !block.is_null() {
            let start = block as usize;
            let end = start + unsafe { (*block).size };
            let next = unsafe { (*block).next };
            let payload = align_up(start + HEADER, align);
            let front = payload - start - HEADER;
            // The gap before an over-aligned payload must become its own block.
            if (front == 0 || front >= MIN_ALIGN) && payload + size <= end {
                let header = payload - HEADER;
                // A tail too small to stand as a block of its own goes with it.
                let (tail, rest) = if end - payload - size < MIN_BLOCK {
                    (end, next)
                } else {
                    let tail = payload + size;
                    (tail, unsafe { write_block(tail, end - tail, next) })
                };
                if front == 0 {
                    self.relink(prev, rest);
                } else {
                    unsafe { write_block(start, header - start, rest) };
                }
                unsafe { write_block(header, tail - header, ptr::null_mut()) };
                self.release();
                return payload as *mut u8;
            }
            prev = block;
            block = next;
        }
        self.release();
        ptr::null_mut()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        // Each allocation carries a header, so the layout is not needed here.
        let mut node = (ptr as usize - HEADER) as *mut Block;
        self.acquire();
        let mut prev = ptr::null_mut();
        let mut next = self.head.get();
        while !next.is_null() && (next as usize) < node as usize {
            prev = next;
            next = unsafe { (*next).next };
        }
        // Merge with a neighbour that ends (starts) exactly at this block.
        if !prev.is_null() && unsafe { prev as usize + (*prev).size } == node as usize {
            unsafe { (*prev).size += (*node).size };
            node = prev;
        }
        if !next.is_null() && unsafe { node as usize + (*node).size } == next as usize {
            unsafe { (*node).size += (*next).size };
            next = unsafe { (*next).next };
        }
        unsafe { (*node).next = next };
        if node != prev {
            self.relink(prev, node);
        }
        self.release();
    }
}

/// Stamp a block header at `addr`; the body of the block is left alone.
unsafe fn write_block(addr: usize, size: usize, next: *mut Block) -> *mut Block {
    let block = addr as *mut Block;
    unsafe {
        (*block).size = size;
        (*block).next = next;
    }
    block
}

/// Round `value` up to the next multiple of the power of two `align`.
const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}
