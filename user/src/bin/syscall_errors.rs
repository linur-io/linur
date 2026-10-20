// SPDX-License-Identifier: GPL-3.0-only
#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use core::arch::asm;
use user_lib::{
    OpenFlags, close, exec, exit, fork, mutex_create, open, semaphore_create, thread_create,
    waittid, write,
};

const ENOSYS: isize = -38;
const EINVAL: isize = -22;
const EFAULT: isize = -14;

const SYSCALL_OPEN: usize = 56;
const SYSCALL_PIPE: usize = 59;
const SYSCALL_READ: usize = 63;
const SYSCALL_WRITE: usize = 64;
const SYSCALL_EXEC: usize = 221;
const SYSCALL_WAITTID: usize = 1002;
const SYSCALL_MUTEX_LOCK: usize = 1011;
const SYSCALL_MUTEX_UNLOCK: usize = 1012;
const SYSCALL_SEMAPHORE_UP: usize = 1021;
const SYSCALL_SEMAPHORE_DOWN: usize = 1022;
const SYSCALL_CONDVAR_SIGNAL: usize = 1031;

/// Unmapped user virtual address.
const BAD: usize = 0xdead_0000;
/// 40 'a's plus NUL; the on-disk name limit is 27.
const LONG_NAME: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\0";
/// The trampoline page: mapped in every page table, but kernel-only.
const TRAMPOLINE: usize = usize::MAX - 4095;

/// Raw ecall, bypassing the user_lib wrappers that hide ids.
fn raw(id: usize, a0: usize, a1: usize, a2: usize) -> isize {
    let mut ret: isize;
    unsafe {
        asm!(
            "ecall",
            inlateout("x10") a0 => ret,
            in("x11") a1,
            in("x12") a2,
            in("x17") id
        );
    }
    ret
}

/// Record one expectation and count the matches.
fn check(name: &str, got: isize, want: isize, passed: &mut usize) {
    if got == want {
        println!("syscall_errors: ok {}", name);
        *passed += 1;
    } else {
        println!("syscall_errors: FAIL {}: got {}, want {}", name, got, want);
    }
}

/// Runs the 15 negative cases; returns 0 only if all match.
#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let mut passed = 0usize;
    let argv = [core::ptr::null::<u8>()];

    check("unknown syscall id", raw(999, 0, 0, 0), ENOSYS, &mut passed);
    check(
        "write bad pointer",
        raw(SYSCALL_WRITE, 1, BAD, 1),
        EFAULT,
        &mut passed,
    );
    check(
        "read bad pointer",
        raw(SYSCALL_READ, 0, BAD, 1),
        EFAULT,
        &mut passed,
    );
    check(
        "pipe bad pointer",
        raw(SYSCALL_PIPE, BAD, 0, 0),
        EFAULT,
        &mut passed,
    );
    check(
        "exec bad path",
        raw(SYSCALL_EXEC, BAD, argv.as_ptr() as usize, 0),
        EFAULT,
        &mut passed,
    );
    check(
        "write trampoline",
        raw(SYSCALL_WRITE, 1, TRAMPOLINE, 1),
        EFAULT,
        &mut passed,
    );
    check(
        "read into a read-only page",
        raw(SYSCALL_READ, 0, b"rodata\0".as_ptr() as usize, 1),
        EFAULT,
        &mut passed,
    );
    check(
        "open unknown flags",
        raw(
            SYSCALL_OPEN,
            b"hello_world\0".as_ptr() as usize,
            0xffff_ffff,
            0,
        ),
        EINVAL,
        &mut passed,
    );
    check(
        "mutex_lock no such mutex",
        raw(SYSCALL_MUTEX_LOCK, 999, 0, 0),
        EINVAL,
        &mut passed,
    );
    check(
        "mutex_unlock no such mutex",
        raw(SYSCALL_MUTEX_UNLOCK, 999, 0, 0),
        EINVAL,
        &mut passed,
    );
    check(
        "semaphore_down no such semaphore",
        raw(SYSCALL_SEMAPHORE_DOWN, 999, 0, 0),
        EINVAL,
        &mut passed,
    );
    check(
        "condvar_signal no such condvar",
        raw(SYSCALL_CONDVAR_SIGNAL, 999, 0, 0),
        EINVAL,
        &mut passed,
    );
    check(
        "waittid no such thread",
        raw(SYSCALL_WAITTID, 999, 0, 0),
        -1,
        &mut passed,
    );

    check(
        "open 40-byte name",
        open(LONG_NAME, OpenFlags::CREATE | OpenFlags::WRONLY),
        -1,
        &mut passed,
    );

    let fd = open("badelf\0", OpenFlags::CREATE | OpenFlags::WRONLY);
    write(fd as usize, b"not an elf");
    close(fd as usize);
    check(
        "exec malformed elf",
        exec("badelf\0", &argv),
        EINVAL,
        &mut passed,
    );

    // Releasing something never held must not panic: it changes nothing.
    let mutex = mutex_create();
    check(
        "unlock an unheld mutex",
        raw(SYSCALL_MUTEX_UNLOCK, mutex as usize, 0, 0),
        0,
        &mut passed,
    );
    let sem = semaphore_create(1);
    check(
        "up an undowned semaphore",
        raw(SYSCALL_SEMAPHORE_UP, sem as usize, 0, 0),
        0,
        &mut passed,
    );

    let tid = thread_create(linker_symbol_addr!(entry), 0);
    check(
        "fork and exec in thread",
        waittid(tid as usize),
        0,
        &mut passed,
    );

    println!("syscall_errors: {}/{} passed", passed, 18);
    if passed == 18 { 0 } else { 1 }
}

/// Thread body: fork and exec outside the main thread must fail with EINVAL.
fn entry() -> ! {
    let fork_err = fork() == EINVAL;
    let exec_err = exec("hello_world\0", &[core::ptr::null::<u8>()]) == EINVAL;
    exit(if fork_err && exec_err { 0 } else { 1 })
}
