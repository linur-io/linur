// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

extern crate user_lib;

/// Exit code spawn1 expects from wait.
const EXIT_CODE: i32 = 66778;

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    EXIT_CODE
}
