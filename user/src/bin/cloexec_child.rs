// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{EBADF, Stat, errno, fstat};

/// Spawned by cloexec_test with exec: fd 3 was opened O_CLOEXEC and must be
/// closed, fd 4 was not and must survive. Exits 0 when both hold.
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut st = Stat::default();
    let closed = fstat(3, &mut st) == -1 && errno() == EBADF;
    let survived = fstat(4, &mut st) == 0;
    if closed && survived {
        println!("cloexec_child: fd 3 closed, fd 4 open");
        0
    } else {
        println!(
            "cloexec_child: FAIL fd3_closed={} fd4_open={}",
            closed, survived
        );
        1
    }
}
