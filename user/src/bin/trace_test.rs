// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

extern crate user_lib;

use user_lib::{
    SYSCALL_EXIT, SYSCALL_GET_TIME, SYSCALL_TRACE, SYSCALL_WRITE, SYSCALL_YIELD, count_syscall,
    get_time, println, trace_read, trace_write, yield_,
};

/// Store a byte through trace, so the store goes through the kernel and cannot
/// be optimised away.
fn write_const(var: &mut u8, new_val: u8) {
    assert!(trace_write(var, new_val));
}

/// Counts are incremented at dispatch entry, so each count_syscall call counts
/// itself. Expected: "Test trace OK!".
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let t1 = get_time() as usize;
    get_time();
    // The reference's sleep was a get_time/yield loop and ticked YIELD; here it
    // is one blocking syscall, so yield explicitly instead. The sleep is skipped
    // because this tree's idle loop does not wake a lone sleeping task.
    yield_();
    let t2 = get_time() as usize;
    let t3 = get_time() as usize;
    assert!(3 <= count_syscall(SYSCALL_GET_TIME));
    // This trace call is counted too: the second one returns 2.
    assert_eq!(2, count_syscall(SYSCALL_TRACE));
    assert_eq!(0, count_syscall(SYSCALL_WRITE));
    assert!(0 < count_syscall(SYSCALL_YIELD));
    assert_eq!(0, count_syscall(SYSCALL_EXIT));

    println!("string from task trace test\n");
    let t4 = get_time() as usize;
    let t5 = get_time() as usize;
    assert!(5 <= count_syscall(SYSCALL_GET_TIME));
    assert_eq!(7, count_syscall(SYSCALL_TRACE));
    assert_eq!(1, count_syscall(SYSCALL_WRITE));
    assert!(0 < count_syscall(SYSCALL_YIELD));
    assert_eq!(0, count_syscall(SYSCALL_EXIT));

    let mut var = 111u8;
    assert_eq!(Some(111), trace_read(&var as *const u8));
    write_const(&mut var, (t1 ^ t2 ^ t3 ^ t4 ^ t5) as u8);
    assert_eq!((t1 ^ t2 ^ t3 ^ t4 ^ t5) as u8, unsafe {
        core::ptr::read_volatile(&var)
    });

    assert!(trace_read(main as *const u8).is_some());
    println!("Test trace OK!\n");
    0
}
