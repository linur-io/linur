// SPDX-License-Identifier: GPL-3.0-only
#![no_std]

// Syscall numbers the trace test asserts on (docs/reland.md §2).
pub const SYSCALL_WRITE: usize = 64;
pub const SYSCALL_EXIT: usize = 93;
pub const SYSCALL_YIELD: usize = 124;
pub const SYSCALL_GET_TIME: usize = 169;
pub const SYSCALL_TRACE: usize = 1041;

#[macro_export]
macro_rules! linker_symbol_addr {
    ($symbol:path) => {
        ($symbol as *const ()).addr()
    };
}

#[macro_use]
pub mod console;
mod errno;
mod lang_items;
mod syscall;

pub use errno::{
    E2BIG, EAGAIN, EBADF, ECHILD, EDEADLK, EFAULT, EINVAL, ENAMETOOLONG, ENOENT, ENOEXEC, ENOMEM,
    ENOSYS, ESRCH, errno,
};

extern crate alloc;
#[macro_use]
extern crate bitflags;

use alloc::vec::Vec;
use buddy_system_allocator::LockedHeap;
use core::ptr::addr_of_mut;
use syscall::*;

unsafe extern "Rust" {
    fn main(argc: usize, argv: &[&str]) -> i32;
}

const USER_HEAP_SIZE: usize = 32768;

static mut HEAP_SPACE: [u8; USER_HEAP_SIZE] = [0; USER_HEAP_SIZE];

#[global_allocator]
static HEAP: LockedHeap = LockedHeap::empty();

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
pub extern "C" fn _start(argc: usize, argv: usize) -> ! {
    unsafe {
        HEAP.lock()
            .init(addr_of_mut!(HEAP_SPACE) as usize, USER_HEAP_SIZE);
    }
    let mut v: Vec<&'static str> = Vec::new();
    for i in 0..argc {
        let str_start =
            unsafe { ((argv + i * core::mem::size_of::<usize>()) as *const usize).read_volatile() };
        let len = (0usize..)
            .find(|i| unsafe { ((str_start + *i) as *const u8).read_volatile() == 0 })
            .unwrap();
        v.push(
            core::str::from_utf8(unsafe {
                core::slice::from_raw_parts(str_start as *const u8, len)
            })
            .unwrap(),
        );
    }
    unsafe {
        exit(main(argc, v.as_slice()));
    }
}

bitflags! {
    pub struct OpenFlags: u32 {
        const RDONLY = 0;
        const WRONLY = 1 << 0;
        const RDWR = 1 << 1;
        const CREATE = 1 << 9;
        const TRUNC = 1 << 10;
    }
}

pub fn dup(fd: usize) -> isize {
    sys_dup(fd)
}
pub fn dup2(oldfd: usize, newfd: usize) -> isize {
    sys_dup2(oldfd, newfd)
}
pub fn open(path: &str, flags: OpenFlags) -> isize {
    sys_open(path, flags.bits)
}
pub fn close(fd: usize) -> isize {
    sys_close(fd)
}
pub fn pipe(pipe_fd: &mut [usize]) -> isize {
    sys_pipe(pipe_fd)
}
pub fn read(fd: usize, buf: &mut [u8]) -> isize {
    sys_read(fd, buf)
}
pub fn write(fd: usize, buf: &[u8]) -> isize {
    sys_write(fd, buf)
}
pub fn exit(exit_code: i32) -> ! {
    sys_exit(exit_code);
}
pub fn yield_() -> isize {
    sys_yield()
}
pub fn get_time() -> isize {
    sys_get_time()
}
pub fn getpid() -> isize {
    sys_getpid()
}
pub fn fork() -> isize {
    sys_fork()
}
pub fn exec(path: &str, args: &[*const u8]) -> isize {
    sys_exec(path, args)
}
/// Create a process from the image at `path` and return its pid.
pub fn spawn(path: &str) -> isize {
    sys_spawn(path)
}
/// Set the program break and return it; `brk(0)` only reads the current one.
pub fn brk(addr: usize) -> isize {
    sys_brk(addr)
}
/// Map `len` bytes at the fixed address `start`; `prot` is R/W/X = 1/2/4.
pub fn mmap(start: usize, len: usize, prot: usize) -> isize {
    sys_mmap(start, len, prot)
}
/// Unmap the `len` bytes at `start`.
pub fn munmap(start: usize, len: usize) -> isize {
    sys_munmap(start, len)
}
/// Reap any child, blocking by yielding while it still runs.
pub fn wait(exit_code: &mut i32) -> isize {
    loop {
        let exit_pid = sys_waitpid(-1, exit_code as *mut _);
        if exit_pid == -1 && errno() == EAGAIN {
            yield_();
            continue;
        }
        // -1 on any other error, or the pid of the reaped child
        return exit_pid;
    }
}

/// Reap one child, blocking by yielding while it still runs.
pub fn waitpid(pid: usize, exit_code: &mut i32) -> isize {
    loop {
        let exit_pid = sys_waitpid(pid as isize, exit_code as *mut _);
        if exit_pid == -1 && errno() == EAGAIN {
            yield_();
            continue;
        }
        // -1 on any other error, or the pid of the reaped child
        return exit_pid;
    }
}

pub fn waitpid_nb(pid: usize, exit_code: &mut i32) -> isize {
    sys_waitpid(pid as isize, exit_code as *mut _)
}

bitflags! {
    pub struct SignalFlags: i32 {
        const SIGINT    = 1 << 2;
        const SIGILL    = 1 << 4;
        const SIGABRT   = 1 << 6;
        const SIGFPE    = 1 << 8;
        const SIGSEGV   = 1 << 11;
    }
}

pub fn kill(pid: usize, signal: i32) -> isize {
    sys_kill(pid, signal)
}

pub fn sleep(sleep_ms: usize) {
    sys_sleep(sleep_ms);
}

/// Set this task's scheduler weight and return it.
pub fn set_priority(prio: isize) -> isize {
    sys_set_priority(prio)
}

pub fn thread_create(entry: usize, arg: usize) -> isize {
    sys_thread_create(entry, arg)
}
pub fn gettid() -> isize {
    sys_gettid()
}
/// Join a thread, blocking by yielding while it still runs. Returns its exit
/// code, or -1 with `errno()` set.
pub fn waittid(tid: usize) -> isize {
    let mut status: i32 = 0;
    loop {
        let ret = sys_waittid(tid, &mut status);
        if ret == -1 && errno() == EAGAIN {
            yield_();
            continue;
        }
        if ret < 0 {
            return ret;
        }
        return status as isize;
    }
}

pub fn enable_deadlock_detect(is_enable: bool) -> isize {
    sys_enable_deadlock_detect(is_enable)
}

pub fn mutex_create() -> isize {
    sys_mutex_create(false)
}
pub fn mutex_blocking_create() -> isize {
    sys_mutex_create(true)
}
pub fn mutex_lock(mutex_id: usize) -> isize {
    sys_mutex_lock(mutex_id)
}
pub fn mutex_unlock(mutex_id: usize) {
    sys_mutex_unlock(mutex_id);
}
pub fn semaphore_create(res_count: usize) -> isize {
    sys_semaphore_create(res_count)
}
pub fn semaphore_up(sem_id: usize) {
    sys_semaphore_up(sem_id);
}
pub fn semaphore_down(sem_id: usize) -> isize {
    sys_semaphore_down(sem_id)
}
pub fn condvar_create() -> isize {
    sys_condvar_create()
}
pub fn condvar_signal(condvar_id: usize) {
    sys_condvar_signal(condvar_id);
}
pub fn condvar_wait(condvar_id: usize, mutex_id: usize) {
    sys_condvar_wait(condvar_id, mutex_id);
}

/// Read the byte at `addr`, or `None` when the address is inaccessible.
pub fn trace_read(addr: *const u8) -> Option<isize> {
    let byte = sys_trace(0, addr as usize, 0);
    if byte < 0 { None } else { Some(byte) }
}

/// Write `data` at `addr`; false when the address is inaccessible.
pub fn trace_write(addr: *mut u8, data: u8) -> bool {
    sys_trace(1, addr as usize, data as usize) == 0
}

/// Number of calls to syscall `id` made by this task, this one included.
pub fn count_syscall(id: usize) -> isize {
    sys_trace(2, id, 0)
}

#[macro_export]
macro_rules! vstore {
    ($var: expr, $value: expr) => {
        // unsafe { core::intrinsics::volatile_store($var_ref as *const _ as _, $value) }
        unsafe {
            core::ptr::write_volatile(core::ptr::addr_of_mut!($var), $value);
        }
    };
}

#[macro_export]
macro_rules! vload {
    ($var: expr) => {
        // unsafe { core::intrinsics::volatile_load($var_ref as *const _ as _) }
        unsafe { core::ptr::read_volatile(core::ptr::addr_of!($var)) }
    };
}

#[macro_export]
macro_rules! memory_fence {
    () => {
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst)
    };
}
