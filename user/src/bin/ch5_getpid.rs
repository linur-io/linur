// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::getpid;

/// Target of spawn0: print the pid the kernel handed this process.
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let pid = getpid();
    println!("Test getpid OK! pid = {}", pid);
    0
}
