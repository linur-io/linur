// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

extern crate user_lib;

use user_lib::{set_priority, spawn, waitpid};

static TESTS: &[&str] = &[
    "ch5_stride0\0",
    "ch5_stride1\0",
    "ch5_stride2\0",
    "ch5_stride3\0",
    "ch5_stride4\0",
    "ch5_stride5\0",
];

/// Spawn six children that each spin for four seconds with a different weight,
/// lower this process's weight, then reap all six. The children print their
/// loop count over their weight; the ratios must be roughly equal.
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut pid = [0; 6];
    for (i, test) in TESTS.iter().enumerate() {
        pid[i] = spawn(*test);
    }
    set_priority(4);
    for i in 0..6 {
        let mut xstate: i32 = Default::default();
        let wait_pid = waitpid(pid[i] as usize, &mut xstate);
        assert_eq!(pid[i], wait_pid);
    }
    0
}
