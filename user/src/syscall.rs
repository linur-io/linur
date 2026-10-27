// SPDX-License-Identifier: GPL-3.0-only
use crate::Stat;
use crate::errno;
use core::arch::asm;

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

/// Enter the kernel and turn `-errno` into `-1` plus [`errno`].
fn syscall(id: usize, args: [usize; 6]) -> isize {
    let mut ret: isize;
    unsafe {
        asm!(
            "ecall",
            inlateout("x10") args[0] => ret,
            in("x11") args[1],
            in("x12") args[2],
            in("x13") args[3],
            in("x14") args[4],
            in("x15") args[5],
            in("x17") id
        );
    }
    if ret < 0 {
        errno::set(-ret);
        -1
    } else {
        ret
    }
}

pub fn sys_getcwd(buf: *mut u8, size: usize) -> isize {
    syscall(SYSCALL_GETCWD, [buf as usize, size, 0, 0, 0, 0])
}

pub fn sys_dup(fd: usize) -> isize {
    syscall(SYSCALL_DUP, [fd, 0, 0, 0, 0, 0])
}

pub fn sys_mkdir(path: &str, mode: u32) -> isize {
    syscall(
        SYSCALL_MKDIR,
        [path.as_ptr() as usize, mode as usize, 0, 0, 0, 0],
    )
}

pub fn sys_mknod(path: &str, mode: u32, dev: u32) -> isize {
    syscall(
        SYSCALL_MKNOD,
        [path.as_ptr() as usize, mode as usize, dev as usize, 0, 0, 0],
    )
}

pub fn sys_unlink(path: &str, flags: u32) -> isize {
    syscall(
        SYSCALL_UNLINK,
        [path.as_ptr() as usize, flags as usize, 0, 0, 0, 0],
    )
}

pub fn sys_link(old: &str, new: &str) -> isize {
    syscall(
        SYSCALL_LINK,
        [old.as_ptr() as usize, new.as_ptr() as usize, 0, 0, 0, 0],
    )
}

pub fn sys_rename(old: &str, new: &str) -> isize {
    syscall(
        SYSCALL_RENAME,
        [old.as_ptr() as usize, new.as_ptr() as usize, 0, 0, 0, 0],
    )
}

pub fn sys_chdir(path: &str) -> isize {
    syscall(SYSCALL_CHDIR, [path.as_ptr() as usize, 0, 0, 0, 0, 0])
}

pub fn sys_access(path: &str, mode: u32) -> isize {
    syscall(
        SYSCALL_ACCESS,
        [path.as_ptr() as usize, mode as usize, 0, 0, 0, 0],
    )
}

pub fn sys_chmod(path: &str, mode: u32) -> isize {
    syscall(
        SYSCALL_CHMOD,
        [path.as_ptr() as usize, mode as usize, 0, 0, 0, 0],
    )
}

pub fn sys_chown(path: &str, uid: u32, gid: u32) -> isize {
    syscall(
        SYSCALL_CHOWN,
        [path.as_ptr() as usize, uid as usize, gid as usize, 0, 0, 0],
    )
}

pub fn sys_open(path: &str, flags: u32, mode: u32) -> isize {
    syscall(
        SYSCALL_OPEN,
        [
            path.as_ptr() as usize,
            flags as usize,
            mode as usize,
            0,
            0,
            0,
        ],
    )
}

pub fn sys_close(fd: usize) -> isize {
    syscall(SYSCALL_CLOSE, [fd, 0, 0, 0, 0, 0])
}

pub fn sys_pipe(pipe: &mut [usize]) -> isize {
    syscall(SYSCALL_PIPE, [pipe.as_mut_ptr() as usize, 0, 0, 0, 0, 0])
}

pub fn sys_getdents(fd: usize, buf: *mut u8, len: usize) -> isize {
    syscall(SYSCALL_GETDENTS64, [fd, buf as usize, len, 0, 0, 0])
}

pub fn sys_lseek(fd: usize, offset: isize, whence: u32) -> isize {
    syscall(
        SYSCALL_LSEEK,
        [fd, offset as usize, whence as usize, 0, 0, 0],
    )
}

pub fn sys_read(fd: usize, buffer: &mut [u8]) -> isize {
    syscall(
        SYSCALL_READ,
        [fd, buffer.as_mut_ptr() as usize, buffer.len(), 0, 0, 0],
    )
}

pub fn sys_write(fd: usize, buffer: &[u8]) -> isize {
    syscall(
        SYSCALL_WRITE,
        [fd, buffer.as_ptr() as usize, buffer.len(), 0, 0, 0],
    )
}

pub fn sys_stat(path: &str, stat: *mut Stat) -> isize {
    syscall(
        SYSCALL_STAT,
        [path.as_ptr() as usize, stat as usize, 0, 0, 0, 0],
    )
}

pub fn sys_fstat(fd: usize, stat: *mut Stat) -> isize {
    syscall(SYSCALL_FSTAT, [fd, stat as usize, 0, 0, 0, 0])
}

pub fn sys_exit(exit_code: i32) -> ! {
    syscall(SYSCALL_EXIT, [exit_code as usize, 0, 0, 0, 0, 0]);
    panic!("sys_exit never returns!");
}

pub fn sys_sleep(sleep_ms: usize) -> isize {
    syscall(SYSCALL_SLEEP, [sleep_ms, 0, 0, 0, 0, 0])
}

pub fn sys_yield() -> isize {
    syscall(SYSCALL_YIELD, [0, 0, 0, 0, 0, 0])
}

pub fn sys_kill(pid: usize, signal: i32) -> isize {
    syscall(SYSCALL_KILL, [pid, signal as usize, 0, 0, 0, 0])
}

pub fn sys_umask(mask: u32) -> isize {
    syscall(SYSCALL_UMASK, [mask as usize, 0, 0, 0, 0, 0])
}

pub fn sys_get_time() -> isize {
    syscall(SYSCALL_GET_TIME, [0, 0, 0, 0, 0, 0])
}

pub fn sys_getpid() -> isize {
    syscall(SYSCALL_GETPID, [0, 0, 0, 0, 0, 0])
}

pub fn sys_brk(addr: usize) -> isize {
    syscall(SYSCALL_BRK, [addr, 0, 0, 0, 0, 0])
}

pub fn sys_munmap(start: usize, len: usize) -> isize {
    syscall(SYSCALL_MUNMAP, [start, len, 0, 0, 0, 0])
}

pub fn sys_fork() -> isize {
    syscall(SYSCALL_FORK, [0, 0, 0, 0, 0, 0])
}

pub fn sys_exec(path: &str, args: *const *const u8, envs: *const *const u8) -> isize {
    syscall(
        SYSCALL_EXEC,
        [
            path.as_ptr() as usize,
            args as usize,
            envs as usize,
            0,
            0,
            0,
        ],
    )
}

pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    syscall(SYSCALL_MMAP, [start, len, prot, 0, 0, 0])
}

pub fn sys_waitpid(pid: isize, exit_code: *mut i32) -> isize {
    syscall(
        SYSCALL_WAITPID,
        [pid as usize, exit_code as usize, 0, 0, 0, 0],
    )
}

pub fn sys_enable_deadlock_detect(is_enable: bool) -> isize {
    syscall(
        SYSCALL_ENABLE_DEADLOCK_DETECT,
        [is_enable as usize, 0, 0, 0, 0, 0],
    )
}

pub fn sys_thread_create(entry: usize, arg: usize) -> isize {
    syscall(SYSCALL_THREAD_CREATE, [entry, arg, 0, 0, 0, 0])
}

pub fn sys_gettid() -> isize {
    syscall(SYSCALL_GETTID, [0; 6])
}

pub fn sys_waittid(tid: usize, exit_code: *mut i32) -> isize {
    syscall(SYSCALL_WAITTID, [tid, exit_code as usize, 0, 0, 0, 0])
}

pub fn sys_mutex_create(blocking: bool) -> isize {
    syscall(SYSCALL_MUTEX_CREATE, [blocking as usize, 0, 0, 0, 0, 0])
}

pub fn sys_mutex_lock(id: usize) -> isize {
    syscall(SYSCALL_MUTEX_LOCK, [id, 0, 0, 0, 0, 0])
}

pub fn sys_mutex_unlock(id: usize) -> isize {
    syscall(SYSCALL_MUTEX_UNLOCK, [id, 0, 0, 0, 0, 0])
}

pub fn sys_semaphore_create(res_count: usize) -> isize {
    syscall(SYSCALL_SEMAPHORE_CREATE, [res_count, 0, 0, 0, 0, 0])
}

pub fn sys_semaphore_up(sem_id: usize) -> isize {
    syscall(SYSCALL_SEMAPHORE_UP, [sem_id, 0, 0, 0, 0, 0])
}

pub fn sys_semaphore_down(sem_id: usize) -> isize {
    syscall(SYSCALL_SEMAPHORE_DOWN, [sem_id, 0, 0, 0, 0, 0])
}

pub fn sys_condvar_create() -> isize {
    syscall(SYSCALL_CONDVAR_CREATE, [0, 0, 0, 0, 0, 0])
}

pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    syscall(SYSCALL_CONDVAR_SIGNAL, [condvar_id, 0, 0, 0, 0, 0])
}

pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    syscall(SYSCALL_CONDVAR_WAIT, [condvar_id, mutex_id, 0, 0, 0, 0])
}

pub fn sys_spawn(path: &str) -> isize {
    syscall(SYSCALL_SPAWN, [path.as_ptr() as usize, 0, 0, 0, 0, 0])
}

pub fn sys_trace(request: usize, id: usize, data: usize) -> isize {
    syscall(SYSCALL_TRACE, [request, id, data, 0, 0, 0])
}

pub fn sys_set_priority(priority: isize) -> isize {
    syscall(SYSCALL_SET_PRIORITY, [priority as usize, 0, 0, 0, 0, 0])
}

pub fn sys_dup2(oldfd: usize, newfd: usize) -> isize {
    syscall(SYSCALL_DUP2, [oldfd, newfd, 0, 0, 0, 0])
}
