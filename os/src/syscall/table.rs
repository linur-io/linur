// SPDX-License-Identifier: GPL-3.0-only
//! The riscv64 syscall table.
//!
//! Numbers are architecture-specific: riscv64 numbers `open` 56 and `fork` 220
//! where x86_64 numbers them 2 and 57, so this number→handler table and the
//! calling convention live per architecture. The handlers in the parent module
//! are the shared semantic core; x86_64 gets its sibling table with M5.4.

use super::fs::*;
use super::mm::*;
use super::process::*;
use super::sched::*;
use super::sync::*;
use super::thread::*;
use crate::errno::ENOSYS;

const SYSCALL_GETCWD: usize = 17;
const SYSCALL_DUP: usize = 24;
const SYSCALL_MKNOD: usize = 33;
const SYSCALL_MKDIR: usize = 34;
const SYSCALL_UNLINK: usize = 35;
const SYSCALL_LINK: usize = 37;
const SYSCALL_RENAME: usize = 38;
const SYSCALL_ACCESS: usize = 48;
const SYSCALL_CHDIR: usize = 49;
const SYSCALL_CHMOD: usize = 53;
const SYSCALL_CHOWN: usize = 54;
const SYSCALL_OPEN: usize = 56;
const SYSCALL_CLOSE: usize = 57;
const SYSCALL_PIPE: usize = 59;
const SYSCALL_GETDENTS64: usize = 61;
const SYSCALL_LSEEK: usize = 62;
const SYSCALL_READ: usize = 63;
const SYSCALL_WRITE: usize = 64;
const SYSCALL_STAT: usize = 79;
const SYSCALL_FSTAT: usize = 80;
const SYSCALL_EXIT: usize = 93;
const SYSCALL_SLEEP: usize = 101;
const SYSCALL_YIELD: usize = 124;
const SYSCALL_KILL: usize = 129;
const SYSCALL_UMASK: usize = 166;
const SYSCALL_GET_TIME: usize = 169;
const SYSCALL_GETPID: usize = 172;
const SYSCALL_BRK: usize = 214;
const SYSCALL_MUNMAP: usize = 215;
const SYSCALL_FORK: usize = 220;
const SYSCALL_EXEC: usize = 221;
const SYSCALL_MMAP: usize = 222;
const SYSCALL_WAITPID: usize = 260;
const SYSCALL_ENABLE_DEADLOCK_DETECT: usize = 469;
const SYSCALL_THREAD_CREATE: usize = 1000;
const SYSCALL_GETTID: usize = 1001;
const SYSCALL_WAITTID: usize = 1002;
const SYSCALL_MUTEX_CREATE: usize = 1010;
const SYSCALL_MUTEX_LOCK: usize = 1011;
const SYSCALL_MUTEX_UNLOCK: usize = 1012;
const SYSCALL_SEMAPHORE_CREATE: usize = 1020;
const SYSCALL_SEMAPHORE_UP: usize = 1021;
const SYSCALL_SEMAPHORE_DOWN: usize = 1022;
const SYSCALL_CONDVAR_CREATE: usize = 1030;
const SYSCALL_CONDVAR_SIGNAL: usize = 1031;
const SYSCALL_CONDVAR_WAIT: usize = 1032;
const SYSCALL_SPAWN: usize = 1040;
const SYSCALL_TRACE: usize = 1041;
const SYSCALL_SET_PRIORITY: usize = 1042;
const SYSCALL_DUP2: usize = 1043;

/// Run one syscall: number from a7, up to six arguments from a0-a5.
///
/// Every call is counted here, before dispatch, so `trace` counts the call
/// asking the question and unknown ids are counted too (docs/reland.md §6).
pub fn dispatch(syscall_id: usize, args: [usize; 6]) -> isize {
    crate::task::record_syscall(syscall_id);
    match syscall_id {
        SYSCALL_GETCWD => sys_getcwd(args[0] as *mut u8, args[1]),
        SYSCALL_DUP => sys_dup(args[0]),
        SYSCALL_MKNOD => sys_mknod(args[0] as *const u8, args[1] as u32, args[2] as u32),
        SYSCALL_MKDIR => sys_mkdir(args[0] as *const u8, args[1] as u32),
        SYSCALL_UNLINK => sys_unlink(args[0] as *const u8, args[1] as u32),
        SYSCALL_LINK => sys_link(args[0] as *const u8, args[1] as *const u8),
        SYSCALL_RENAME => sys_rename(args[0] as *const u8, args[1] as *const u8),
        SYSCALL_ACCESS => sys_access(args[0] as *const u8, args[1] as u32),
        SYSCALL_CHDIR => sys_chdir(args[0] as *const u8),
        SYSCALL_CHMOD => sys_chmod(args[0] as *const u8, args[1] as u32),
        SYSCALL_CHOWN => sys_chown(args[0] as *const u8, args[1] as u32, args[2] as u32),
        SYSCALL_OPEN => sys_open(args[0] as *const u8, args[1] as u32, args[2] as u32),
        SYSCALL_CLOSE => sys_close(args[0]),
        SYSCALL_PIPE => sys_pipe(args[0] as *mut usize),
        SYSCALL_GETDENTS64 => sys_getdents64(args[0], args[1] as *mut u8, args[2]),
        SYSCALL_LSEEK => sys_lseek(args[0], args[1] as isize, args[2] as u32),
        SYSCALL_READ => sys_read(args[0], args[1] as *mut u8, args[2]),
        SYSCALL_WRITE => sys_write(args[0], args[1] as *const u8, args[2]),
        SYSCALL_STAT => sys_stat(args[0] as *const u8, args[1] as *mut u8),
        SYSCALL_FSTAT => sys_fstat(args[0], args[1] as *mut u8),
        SYSCALL_EXIT => sys_exit(args[0] as i32),
        SYSCALL_SLEEP => sys_sleep(args[0]),
        SYSCALL_YIELD => sys_yield(),
        SYSCALL_KILL => sys_kill(args[0], args[1] as u32),
        SYSCALL_UMASK => sys_umask(args[0] as u32),
        SYSCALL_GET_TIME => sys_get_time(),
        SYSCALL_GETPID => sys_getpid(),
        SYSCALL_BRK => sys_brk(args[0]),
        SYSCALL_MUNMAP => sys_munmap(args[0], args[1]),
        SYSCALL_FORK => sys_fork(),
        SYSCALL_EXEC => sys_exec(
            args[0] as *const u8,
            args[1] as *const usize,
            args[2] as *const usize,
        ),
        SYSCALL_MMAP => sys_mmap(args[0], args[1], args[2]),
        SYSCALL_WAITPID => sys_waitpid(args[0] as isize, args[1] as *mut i32),
        SYSCALL_ENABLE_DEADLOCK_DETECT => sys_enable_deadlock_detect(args[0]),
        SYSCALL_THREAD_CREATE => sys_thread_create(args[0], args[1]),
        SYSCALL_GETTID => sys_gettid(),
        SYSCALL_WAITTID => sys_waittid(args[0], args[1] as *mut i32),
        SYSCALL_MUTEX_CREATE => sys_mutex_create(args[0] == 1),
        SYSCALL_MUTEX_LOCK => sys_mutex_lock(args[0]),
        SYSCALL_MUTEX_UNLOCK => sys_mutex_unlock(args[0]),
        SYSCALL_SEMAPHORE_CREATE => sys_semaphore_create(args[0]),
        SYSCALL_SEMAPHORE_UP => sys_semaphore_up(args[0]),
        SYSCALL_SEMAPHORE_DOWN => sys_semaphore_down(args[0]),
        SYSCALL_CONDVAR_CREATE => sys_condvar_create(),
        SYSCALL_CONDVAR_SIGNAL => sys_condvar_signal(args[0]),
        SYSCALL_CONDVAR_WAIT => sys_condvar_wait(args[0], args[1]),
        SYSCALL_SPAWN => sys_spawn(args[0] as *const u8),
        SYSCALL_TRACE => sys_trace(args[0], args[1], args[2]),
        SYSCALL_SET_PRIORITY => sys_set_priority(args[0] as isize),
        SYSCALL_DUP2 => sys_dup2(args[0], args[1]),
        _ => ENOSYS,
    }
}
