// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::environ;

const NAME: &str = "envp_child";

/// Check the environment `elf_test` gave this process: `argv[1]` names the case,
/// and the exit code is 0 only when `environ()` matches it (docs/elf.md §7).
#[unsafe(no_mangle)]
pub fn main(_argc: usize, argv: &[&str]) -> i32 {
    let want: &[&str] = match argv[1] {
        "two" => &["A=1", "B=2"],
        "none" => &[],
        other => {
            println!("{}: FAIL argv {}", NAME, other);
            return 1;
        }
    };
    if environ() == want {
        return 0;
    }
    println!("{}: FAIL environ {:?}", NAME, environ());
    1
}
