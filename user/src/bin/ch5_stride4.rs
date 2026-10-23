// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{get_time, set_priority};

/// Keep the CPU busy for a few instructions.
fn spin_delay() {
    let mut j = true;
    for _ in 0..10 {
        j = !j;
    }
}

/// Run until MAX_TIME milliseconds have elapsed and return the loop count.
fn count_during(prio: isize) -> isize {
    let start_time = get_time();
    let mut acc = 0;
    set_priority(prio);
    loop {
        spin_delay();
        acc += 1;
        if acc % 400 == 0 {
            let time = get_time() - start_time;
            if time > MAX_TIME {
                return acc;
            }
        }
    }
}

// To get enough accuracy, MAX_TIME (the running time of each process) should be
// more than 1000 milliseconds.
const MAX_TIME: isize = 4000;

/// One of the six siblings ch5_stride spawns; they differ only in their weight,
/// 5 through 10, so the count over weight ratio must come out roughly equal.
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let prio = 9;
    let count = count_during(prio);
    println!(
        "priority = {}, exitcode = {}, ratio = {}",
        prio,
        count,
        count / prio
    );
    0
}
