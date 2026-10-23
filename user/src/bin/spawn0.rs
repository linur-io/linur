// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{spawn, wait};

const MAX_CHILD: usize = 40;

/// Spawn MAX_CHILD children, reap them all with the expected exit code, and
/// check that wait then reports no child left. Expected: "Test spawn0 OK!".
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    for _ in 0..MAX_CHILD {
        let cpid = spawn("ch5_getpid\0");
        assert!(cpid >= 0, "child pid invalid");
        println!("new child {}", cpid);
    }
    let mut exit_code: i32 = 0;
    for _ in 0..MAX_CHILD {
        assert!(wait(&mut exit_code) > 0, "wait stopped early");
        assert_eq!(exit_code, 0, "error exit code {}", exit_code);
    }
    assert!(wait(&mut exit_code) <= 0, "wait got too many");
    println!("Test spawn0 OK!");
    0
}
