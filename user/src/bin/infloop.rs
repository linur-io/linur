// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]
#![allow(clippy::empty_loop)]

extern crate user_lib;

#[unsafe(no_mangle)]
pub fn main(_argc: usize, _argv: &[&str]) -> ! {
    loop {}
}
