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
    E2BIG, EACCES, EAGAIN, EBADF, ECHILD, EDEADLK, EEXIST, EFAULT, EINVAL, EISDIR, ENAMETOOLONG,
    ENOENT, ENOEXEC, ENOMEM, ENOSYS, ENOTDIR, ENOTEMPTY, ENXIO, EPERM, ERANGE, ESPIPE, ESRCH,
    errno,
};

extern crate alloc;
#[macro_use]
extern crate bitflags;

use alloc::string::String;
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
        const WRONLY = 0x1;
        const RDWR = 0x2;
        const CREATE = 0x40;
        const EXCL = 0x80;
        const TRUNC = 0x200;
        const APPEND = 0x400;
        const DIRECTORY = 0x10000;
        const CLOEXEC = 0x80000;
    }
}

/// The `lseek` starting point.
pub const SEEK_SET: u32 = 0;
/// The `lseek` starting point.
pub const SEEK_CUR: u32 = 1;
/// The `lseek` starting point.
pub const SEEK_END: u32 = 2;

/// `unlink` flag: remove a directory (the `rmdir` wrapper).
pub const AT_REMOVEDIR: u32 = 0x200;

/// `access`: the file exists.
pub const F_OK: u32 = 0;
/// `access`: the file is executable.
pub const X_OK: u32 = 1;
/// `access`: the file is writable.
pub const W_OK: u32 = 2;
/// `access`: the file is readable.
pub const R_OK: u32 = 4;

/// `st_mode` mask for the file type.
pub const S_IFMT: u32 = 0o170000;
/// `st_mode`: regular file.
pub const S_IFREG: u32 = 0o100000;
/// `st_mode`: directory.
pub const S_IFDIR: u32 = 0o040000;
/// `st_mode`: character device.
pub const S_IFCHR: u32 = 0o020000;
/// `st_mode`: block device.
pub const S_IFBLK: u32 = 0o060000;
/// `st_mode`: FIFO (pipe).
pub const S_IFIFO: u32 = 0o010000;

/// Encode a device number as `(major << 8) | minor` (docs/perm.md §3).
pub fn makedev(major: u32, minor: u32) -> u32 {
    (major << 8) | minor
}

/// `d_type`: directory.
pub const DT_DIR: u8 = 4;
/// `d_type`: regular file.
pub const DT_REG: u8 = 8;

/// The Linux riscv64 `struct stat` (docs/vfs.md §8), field for field.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stat {
    pub dev: u64,
    pub ino: u64,
    pub mode: u32,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub rdev: u64,
    pub pad1: u64,
    pub size: i64,
    pub blksize: i32,
    pub pad2: i32,
    pub blocks: i64,
    pub atime: i64,
    pub atime_nsec: i64,
    pub mtime: i64,
    pub mtime_nsec: i64,
    pub ctime: i64,
    pub ctime_nsec: i64,
    pub unused4: u32,
    pub unused5: u32,
}

/// One entry parsed out of a `getdents` buffer.
pub struct DirEntryInfo {
    pub ino: u64,
    pub dtype: u8,
    pub name: String,
}

pub fn dup(fd: usize) -> isize {
    sys_dup(fd)
}
pub fn dup2(oldfd: usize, newfd: usize) -> isize {
    sys_dup2(oldfd, newfd)
}
pub fn open(path: &str, flags: OpenFlags, mode: u32) -> isize {
    sys_open(path, flags.bits, mode)
}
/// `open(path, O_CREAT|O_WRONLY|O_TRUNC, mode)`.
pub fn creat(path: &str, mode: u32) -> isize {
    sys_open(
        path,
        (OpenFlags::CREATE | OpenFlags::WRONLY | OpenFlags::TRUNC).bits,
        mode,
    )
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
/// Read the absolute path of the working directory into `buf`.
pub fn getcwd(buf: &mut [u8]) -> isize {
    sys_getcwd(buf.as_mut_ptr(), buf.len())
}
pub fn chdir(path: &str) -> isize {
    sys_chdir(path)
}
pub fn mkdir(path: &str, mode: u32) -> isize {
    sys_mkdir(path, mode)
}
/// Create a special file; `mode` carries the type, as in `mknod(2)`.
pub fn mknod(path: &str, mode: u32, dev: u32) -> isize {
    sys_mknod(path, mode, dev)
}
/// Remove a directory; `unlinkat(path, AT_REMOVEDIR)`.
pub fn rmdir(path: &str) -> isize {
    sys_unlink(path, AT_REMOVEDIR)
}
pub fn unlink(path: &str) -> isize {
    sys_unlink(path, 0)
}
/// Change a file's permission bits; the type is kept.
pub fn chmod(path: &str, mode: u32) -> isize {
    sys_chmod(path, mode)
}
/// Change a file's owner and group; a `u32::MAX` half is left unchanged.
pub fn chown(path: &str, uid: u32, gid: u32) -> isize {
    sys_chown(path, uid, gid)
}
/// Check access to `path` with the `F_OK`/`R_OK`/`W_OK`/`X_OK` bits.
pub fn access(path: &str, mode: u32) -> isize {
    sys_access(path, mode)
}
/// Set the file creation mask and return the previous one.
pub fn umask(mask: u32) -> u32 {
    sys_umask(mask) as u32
}
pub fn link(old: &str, new: &str) -> isize {
    sys_link(old, new)
}
pub fn rename(old: &str, new: &str) -> isize {
    sys_rename(old, new)
}
pub fn lseek(fd: usize, offset: isize, whence: u32) -> isize {
    sys_lseek(fd, offset, whence)
}
pub fn stat(path: &str, stat: &mut Stat) -> isize {
    sys_stat(path, stat)
}
pub fn fstat(fd: usize, stat: &mut Stat) -> isize {
    sys_fstat(fd, stat)
}
pub fn getdents(fd: usize, buf: &mut [u8]) -> isize {
    sys_getdents(fd, buf.as_mut_ptr(), buf.len())
}
/// Parse the entries out of a `getdents` buffer; `len` is the byte count that
/// `getdents` returned.
pub fn dirents(buf: &[u8], len: usize) -> Vec<DirEntryInfo> {
    // linux_dirent64: ino u64, off i64, reclen u16, dtype u8, name and NUL.
    const HEADER: usize = 19;
    let mut entries = Vec::new();
    let mut offset = 0;
    while offset + HEADER <= len {
        let record = &buf[offset..len];
        let ino = u64::from_ne_bytes(record[0..8].try_into().unwrap());
        let reclen = u16::from_ne_bytes(record[16..18].try_into().unwrap()) as usize;
        let name = &record[HEADER..reclen];
        let name_len = name
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(name.len());
        entries.push(DirEntryInfo {
            ino,
            dtype: record[18],
            name: core::str::from_utf8(&name[..name_len]).unwrap().into(),
        });
        offset += reclen;
    }
    entries
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
