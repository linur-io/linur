// SPDX-License-Identifier: GPL-3.0-only
use crate::config::USER_STACK_SIZE;
use crate::errno::{E2BIG, EACCES, EAGAIN, ECHILD, EINVAL, ENOEXEC, ESRCH};
use crate::fs::{Cred, Dentry, MAY_EXEC, lookup, permission, read_all};
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

/// How many `#!` interpreters may follow each other (Linux uses 4).
const SHEBANG_LIMIT: usize = 4;
/// The longest interpreter line the kernel reads from a script.
const SHEBANG_LINE_MAX: usize = 128;

/// Copy a NULL-terminated vector of user strings, charging the user stack budget
/// (docs/elf.md §4).
fn copy_strings(
    token: usize,
    mut ptr: *const usize,
    budget: &mut usize,
) -> Result<Vec<String>, isize> {
    let mut strings = Vec::new();
    loop {
        let mut slot = [0u8; size_of::<usize>()];
        copy_from_user(token, &mut slot, ptr as *const u8)?;
        let string_addr = usize::from_ne_bytes(slot);
        if string_addr == 0 {
            return Ok(strings);
        }
        let string = translated_str(token, string_addr as *const u8)?;
        *budget += string.len() + 1;
        if *budget + size_of::<usize>() > USER_STACK_SIZE {
            return Err(E2BIG);
        }
        strings.push(string);
        ptr = ptr.wrapping_add(1);
    }
}

/// The interpreter and its optional argument of a `#!` image, or `None` when the
/// image is not a script (docs/elf.md §5).
fn shebang(image: &[u8]) -> Result<Option<(String, Option<String>)>, isize> {
    if !image.starts_with(b"#!") {
        return Ok(None);
    }
    let line = &image[2..image.len().min(2 + SHEBANG_LINE_MAX)];
    let end = line
        .iter()
        .position(|&byte| byte == b'\n')
        .unwrap_or(line.len());
    let Ok(text) = core::str::from_utf8(&line[..end]) else {
        return Err(ENOEXEC);
    };
    let mut words = text.trim().splitn(2, [' ', '\t']);
    let interpreter = words.next().unwrap_or("").trim();
    if interpreter.is_empty() {
        return Err(ENOEXEC);
    }
    let argument = words
        .next()
        .map(|rest| String::from(rest.trim()))
        .filter(|rest| !rest.is_empty());
    Ok(Some((String::from(interpreter), argument)))
}

/// Resolve the image to run and the argv it gets, following `#!` lines
/// (docs/elf.md §5).
fn exec_image(
    cwd: &Arc<Dentry>,
    cred: Cred,
    mut path: String,
    mut args: Vec<String>,
) -> Result<(Vec<u8>, Vec<String>), isize> {
    for _ in 0..SHEBANG_LIMIT {
        let dentry = lookup(cwd, &path, cred)?;
        let inode = dentry.inode();
        // 0.11 answers EACCES for an image that is not a regular file; a real image
        // also has to grant execute permission (docs/perm.md §6).
        if !inode.is_file() {
            return Err(EACCES);
        }
        permission(inode, MAY_EXEC, cred)?;
        let image = read_all(inode);
        let Some((interpreter, argument)) = shebang(&image)? else {
            return Ok((image, args));
        };
        // The interpreter runs with [interp, arg?, script, argv[1..]].
        let mut next = Vec::with_capacity(args.len() + 3);
        next.push(interpreter.clone());
        if let Some(argument) = argument {
            next.push(argument);
        }
        next.push(path.clone());
        next.extend(args.iter().skip(1).cloned());
        path = interpreter;
        args = next;
    }
    Err(ENOEXEC)
}

/// Replace the current image; returns `argc`, which becomes the child's a0.
pub fn sys_exec(path: *const u8, args: *const usize, envp: *const usize) -> isize {
    let token = current_user_token();
    let path = match translated_str(token, path) {
        Ok(path) => path,
        Err(e) => return e,
    };
    // `argv` and `envp` share the user stack budget (docs/elf.md §4).
    let mut budget = 2 * size_of::<usize>();
    let args_vec = match copy_strings(token, args, &mut budget) {
        Ok(args) => args,
        Err(e) => return e,
    };
    // `exec` passes no environment; a null envp is an empty one.
    let envs = if envp.is_null() {
        Vec::new()
    } else {
        match copy_strings(token, envp, &mut budget) {
            Ok(envs) => envs,
            Err(e) => return e,
        }
    };
    // The image is resolved against the working directory, like every path.
    let cwd = current_process().inner_exclusive_access().cwd.clone();
    let cred = Cred::current();
    let (image, args) = match exec_image(&cwd, cred, path, args_vec) {
        Ok(image) => image,
        Err(e) => return e,
    };
    let process = current_process();
    // A new image replaces the one address space, so it cannot be shared
    // with other threads.
    if process.inner_exclusive_access().thread_count() != 1 {
        return EINVAL;
    }
    let argc = args.len();
    if !process.exec(&image, args, envs) {
        return ENOEXEC;
    }
    // return argc because cx.x[10] will be covered with it later
    argc as isize
}

/// Create a child process from the executable at `path`, resolved against the
/// working directory, following `#!` lines; the child starts with an empty argv.
pub fn sys_spawn(path: *const u8) -> isize {
    let token = current_user_token();
    let path = match translated_str(token, path) {
        Ok(path) => path,
        Err(e) => return e,
    };
    let cwd = current_process().inner_exclusive_access().cwd.clone();
    let cred = Cred::current();
    let argv = alloc::vec![path.clone()];
    let (image, _) = match exec_image(&cwd, cred, path, argv) {
        Ok(image) => image,
        Err(e) => return e,
    };
    let Some(child) = current_process().spawn(&image) else {
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
