// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::brk;

/// Grow the heap by two pages and touch every byte, shrink it back and confirm
/// the break, then ask for a break below the heap base and expect -1.
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let base = brk(0);
    assert!(base > 0, "brk(0) failed");
    let base = base as usize;
    let top = base + 8192;
    assert_eq!(brk(top), top as isize);
    for i in 0..8192 {
        unsafe {
            (base as *mut u8).add(i).write_volatile(i as u8);
        }
    }
    for i in 0..8192 {
        assert_eq!(
            unsafe { (base as *const u8).add(i).read_volatile() },
            i as u8
        );
    }
    assert_eq!(brk(base), base as isize);
    assert_eq!(brk(0), base as isize);
    assert_eq!(brk(base - 1), -1);
    println!("Test brk OK!");
    0
}
