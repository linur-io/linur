// SPDX-License-Identifier: GPL-3.0-only
use crate::config::USER_STACK_SIZE;
use crate::errno::{E2BIG, EAGAIN, ECHILD, EINVAL, ENOEXEC, ESRCH};
use crate::fs::{lookup, read_all};
use crate::mm::{copy_from_user, copy_to_user, translated_str};
use crate::task::{
    SignalFlags, current_process, current_task, current_user_token, exit_current_and_run_next,
    pid2process, suspend_current_and_run_next, syscall_count,
};
use crate::timer::get_time_ms;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::mem::size_of;

pub fn sys_exit(exit_code: i32) -> ! {
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    0
}

pub fn sys_get_time() -> isize {
    get_time_ms() as isize
}

pub fn sys_getpid() -> isize {
    current_task().unwrap().process.upgrade().unwrap().getpid() as isize
}

/// A fork copies an address space, so it is refused while the process has more
/// than one thread.
pub fn sys_fork() -> isize {
    let process = current_process();
    if process.inner_exclusive_access().thread_count() != 1 {
        return EINVAL;
    }
    let new_process = process.fork();
    let new_pid = new_process.getpid();
    // modify trap context of new_task, because it returns immediately after switching
    let new_process_inner = new_process.inner_exclusive_access();
    let task = new_process_inner.tasks[0].as_ref().unwrap();
    let trap_cx = task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    new_pid as isize
}

/// Replace the current image; returns `argc`, which becomes the child's a0.
pub fn sys_exec(path: *const u8, mut args: *const usize) -> isize {
    let token = current_user_token();
    let path = match translated_str(token, path) {
        Ok(path) => path,
        Err(e) => return e,
    };
    let mut args_vec: Vec<String> = Vec::new();
    // The argv array and the strings share the user stack, so the stack bounds
    // them; bounding them here also bounds the kernel memory they occupy.
    let mut argv_bytes = size_of::<usize>();
    loop {
        let mut slot = [0u8; size_of::<usize>()];
        if let Err(e) = copy_from_user(token, &mut slot, args as *const u8) {
            return e;
        }
        let arg_str_ptr = usize::from_ne_bytes(slot);
        if arg_str_ptr == 0 {
            break;
        }
        let arg = match translated_str(token, arg_str_ptr as *const u8) {
            Ok(arg) => arg,
            Err(e) => return e,
        };
        argv_bytes += arg.len() + 1 + size_of::<usize>();
        if argv_bytes + size_of::<usize>() > USER_STACK_SIZE {
            return E2BIG;
        }
        args_vec.push(arg);
        unsafe {
            args = args.add(1);
        }
    }
    // The image is resolved against the working directory, like every path.
    let cwd = current_process().inner_exclusive_access().cwd.clone();
    let app = match lookup(&cwd, &path) {
        Ok(app) => app,
        Err(e) => return e,
    };
    let all_data = read_all(app.inode());
    let process = current_process();
    // A new image replaces the one address space, so it cannot be shared
    // with other threads.
    if process.inner_exclusive_access().thread_count() != 1 {
        return EINVAL;
    }
    let argc = args_vec.len();
    if !process.exec(all_data.as_slice(), args_vec) {
        return ENOEXEC;
    }
    // return argc because cx.x[10] will be covered with it later
    argc as isize
}

/// Create a child process from the executable at `path`, resolved against the
/// working directory, with an empty argument list, and return its pid.
pub fn sys_spawn(path: *const u8) -> isize {
    let token = current_user_token();
    let path = match translated_str(token, path) {
        Ok(path) => path,
        Err(e) => return e,
    };
    let cwd = current_process().inner_exclusive_access().cwd.clone();
    let app = match lookup(&cwd, &path) {
        Ok(app) => app,
        Err(e) => return e,
    };
    let all_data = read_all(app.inode());
    let Some(child) = current_process().spawn(all_data.as_slice()) else {
        return ENOEXEC;
    };
    child.getpid() as isize
}

/// Reap one child: returns its pid with the exit code written through
/// `exit_code_ptr`, `-ECHILD` when there is no such child, or `-EAGAIN` while
/// it still runs.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let process = current_process();
    // find a child process
    let mut inner = process.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return ECHILD;
        // ---- release current PCB
    }
    let found = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    let Some((idx, _)) = found else {
        return EAGAIN;
    };
    let found_pid = inner.children[idx].getpid();
    let exit_code = inner.children[idx].inner_exclusive_access().exit_code;
    // The child stays a zombie until its exit code reaches the parent.
    if let Err(e) = copy_to_user(
        inner.memory_set.token(),
        exit_code_ptr as *mut u8,
        &exit_code.to_ne_bytes(),
    ) {
        return e;
    }
    let child = inner.children.remove(idx);
    // confirm that child will be deallocated after being removed from children list
    assert_eq!(Arc::strong_count(&child), 1);
    found_pid as isize
    // ---- release current PCB automatically
}

/// Teaching/debug hook: request 0 reads one user byte at `id`, 1 writes the low
/// byte of `data` there, 2 returns the calling task's count of syscall `id`.
pub fn sys_trace(request: usize, id: usize, data: usize) -> isize {
    let token = current_user_token();
    match request {
        0 => {
            let mut byte = [0u8; 1];
            match copy_from_user(token, &mut byte, id as *const u8) {
                Ok(()) => byte[0] as isize,
                Err(e) => e,
            }
        }
        1 => match copy_to_user(token, id as *mut u8, &[data as u8]) {
            Ok(()) => 0,
            Err(e) => e,
        },
        2 => syscall_count(id),
        _ => EINVAL,
    }
}

pub fn sys_kill(pid: usize, signal: u32) -> isize {
    if let Some(process) = pid2process(pid) {
        if let Some(flag) = SignalFlags::from_bits(signal) {
            process.inner_exclusive_access().signals |= flag;
            0
        } else {
            EINVAL
        }
    } else {
        ESRCH
    }
}
