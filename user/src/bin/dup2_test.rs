// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{close, dup, dup2, write};

/// Three dups grow the descriptor table past 5 so fd 5 is a nameable newfd.
/// Then: dup2 onto an open fd, write through it, bad oldfd, oldfd == newfd,
/// newfd beyond the table, and the grow-by-one rule of docs/reland.md §5.
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    assert!(dup(1) > 0);
    assert!(dup(1) > 0);
    assert!(dup(1) > 0);
    assert_eq!(dup2(1, 5), 5);
    let msg = b"dup2 to fd 5\n";
    assert_eq!(write(5, msg), msg.len() as isize);
    assert_eq!(close(5), 0);
    assert_eq!(dup2(42, 6), -1);
    assert_eq!(dup2(1, 1), 1);
    assert_eq!(dup2(1, 999), -1);
    assert_eq!(dup2(1, 6), 6);
    println!("Test dup2 OK!");
    0
}
