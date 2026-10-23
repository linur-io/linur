// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::mmap;

/// Map one write-only page and load from it: RISC-V has no write-without-read
/// PTE permission, so the load must fault and kill the program. Expected: no
/// "fail" line, the process dies on SIGSEGV.
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let start: usize = 0x1000_0000;
    let len: usize = 4096;
    let prot: usize = 2;
    assert_eq!(0, mmap(start, len, prot));
    let addr: *mut u8 = start as *mut u8;
    unsafe {
        // *addr = start as u8; // can't write, R == 0 && W == 1 is illegal in riscv
        assert!(*addr != 0);
    }
    println!("Should cause error, Test 04_2 fail!");
    0
}
