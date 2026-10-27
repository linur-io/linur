// SPDX-License-Identifier: GPL-3.0-only
//! Host tests for the first-fit heap, over a stack arena.

use core::alloc::{GlobalAlloc, Layout};
use core::mem::size_of;
use heap::LockedHeap;

const ARENA: usize = 1 << 14;

/// Header words: the size and the list link, so 16 bytes on a 64-bit host.
const HEADER: usize = 2 * size_of::<usize>();

/// Three 1024-byte payloads, headers included, exactly fill the tight arena.
const TIGHT: usize = 3 * (1024 + HEADER);

/// A heap arena of `N` bytes, aligned like the payloads it must hold.
#[repr(align(16))]
struct Arena<const N: usize>([u8; N]);

/// A heap over `bytes`, sized to them.
fn heap_on(bytes: &mut [u8]) -> LockedHeap {
    let heap = LockedHeap::empty();
    unsafe { heap.init(bytes.as_mut_ptr() as usize, bytes.len()) };
    heap
}

fn alloc(heap: &LockedHeap, size: usize, align: usize) -> *mut u8 {
    unsafe { heap.alloc(Layout::from_size_align(size, align).unwrap()) }
}

fn dealloc(heap: &LockedHeap, ptr: *mut u8, size: usize, align: usize) {
    unsafe { heap.dealloc(ptr, Layout::from_size_align(size, align).unwrap()) }
}

#[test]
fn fresh_init_then_alloc() {
    let mut arena = Arena([0; ARENA]);
    let heap = heap_on(&mut arena.0);
    let base = arena.0.as_ptr() as usize;
    let ptr = alloc(&heap, 64, 16);
    assert!(!ptr.is_null());
    assert_eq!(ptr as usize % 16, 0);
    assert!(ptr as usize >= base && ptr as usize + 64 <= base + ARENA);
}

#[test]
fn alloc_free_reuse() {
    let mut arena = Arena([0; ARENA]);
    let heap = heap_on(&mut arena.0);
    let first = alloc(&heap, 256, 16);
    dealloc(&heap, first, 256, 16);
    assert_eq!(alloc(&heap, 256, 16), first);
}

#[test]
fn neighbours_coalesce_around_a_freed_middle() {
    let mut arena = Arena([0; TIGHT]);
    let heap = heap_on(&mut arena.0);
    let (a, b, c) = (
        alloc(&heap, 1024, 16),
        alloc(&heap, 1024, 16),
        alloc(&heap, 1024, 16),
    );
    assert!(!a.is_null() && !b.is_null() && !c.is_null());
    assert!(alloc(&heap, 1024, 16).is_null(), "the arena is exhausted");

    dealloc(&heap, b, 1024, 16);
    // One block fits again, two do not: only merging the neighbours can bridge them.
    assert!(alloc(&heap, 1024 + HEADER, 16).is_null());
    assert_eq!(alloc(&heap, 1024, 16), b);
    dealloc(&heap, b, 1024, 16);
    dealloc(&heap, a, 1024, 16);
    dealloc(&heap, c, 1024, 16);
    assert_eq!(alloc(&heap, TIGHT - HEADER, 16), a);
}

#[test]
fn alignment_16_and_64() {
    let mut arena = Arena([0; ARENA]);
    let heap = heap_on(&mut arena.0);
    let plain = alloc(&heap, 64, 16);
    assert_eq!(plain as usize % 16, 0);
    // A small neighbour leaves the next block 16- but not 64-aligned.
    let odd = alloc(&heap, 16, 16);
    let aligned = alloc(&heap, 64, 64);
    assert!(!odd.is_null() && !aligned.is_null());
    assert_eq!(aligned as usize % 64, 0);
    dealloc(&heap, aligned, 64, 64);
    assert_eq!(alloc(&heap, 64, 64) as usize % 64, 0);
}

#[test]
fn unaligned_start_still_yields_aligned_payloads() {
    let mut arena = Arena([0; ARENA]);
    let base = arena.0.as_mut_ptr() as usize;
    let heap = LockedHeap::empty();
    unsafe { heap.init(base + 1, ARENA - 1) };
    let ptr = alloc(&heap, 16, 16);
    assert!(!ptr.is_null());
    assert_eq!(ptr as usize % 16, 0);
    assert!(ptr as usize > base);
}

#[test]
fn oversized_request_is_null() {
    let mut arena = Arena([0; ARENA]);
    let heap = heap_on(&mut arena.0);
    assert!(alloc(&heap, 2 * ARENA, 16).is_null());
}

#[test]
fn randomized_against_a_std_model() {
    let mut arena = Arena([0; ARENA]);
    let heap = heap_on(&mut arena.0);
    let base = arena.0.as_ptr() as usize;
    let mut live: Vec<(*mut u8, usize, usize)> = Vec::new();
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    let mut rng = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..10_000 {
        if rng() % 100 < 55 || live.is_empty() {
            let align = [16, 16, 32, 64][(rng() % 4) as usize];
            let size = 16 * (1 + (rng() % 32) as usize);
            let ptr = alloc(&heap, size, align);
            if ptr.is_null() {
                continue;
            }
            let (start, end) = (ptr as usize, ptr as usize + size);
            assert_eq!(start % align, 0);
            assert!(start >= base && end <= base + ARENA);
            for &(other, other_size, _) in &live {
                let (other_start, other_end) = (other as usize, other as usize + other_size);
                assert!(
                    end <= other_start || other_end <= start,
                    "overlapping blocks"
                );
            }
            live.push((ptr, size, align));
        } else {
            let victim = (rng() % live.len() as u64) as usize;
            let (ptr, size, align) = live.swap_remove(victim);
            dealloc(&heap, ptr, size, align);
        }
    }
}
