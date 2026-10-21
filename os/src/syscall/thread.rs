// SPDX-License-Identifier: GPL-3.0-only
use crate::errno::{EAGAIN, EINVAL, ESRCH};
use crate::{
    mm::{copy_to_user, kernel_token},
    task::{TaskControlBlock, add_task, current_task},
    trap::{TrapContext, trap_handler},
};
use alloc::sync::Arc;

pub fn sys_thread_create(entry: usize, arg: usize) -> isize {
    let task = current_task().unwrap();
    let process = task.process.upgrade().unwrap();
    // create a new thread
    let new_task = Arc::new(TaskControlBlock::new(
        Arc::clone(&process),
        task.inner_exclusive_access()
            .res
            .as_ref()
            .unwrap()
            .ustack_base,
        true,
    ));
    // add new task to scheduler
    add_task(Arc::clone(&new_task));
    let new_task_inner = new_task.inner_exclusive_access();
    let new_task_res = new_task_inner.res.as_ref().unwrap();
    let new_task_tid = new_task_res.tid;
    let mut process_inner = process.inner_exclusive_access();
    // add new thread to current process
    let tasks = &mut process_inner.tasks;
    while tasks.len() < new_task_tid + 1 {
        tasks.push(None);
    }
    tasks[new_task_tid] = Some(Arc::clone(&new_task));
    let new_task_trap_cx = new_task_inner.get_trap_cx();
    *new_task_trap_cx = TrapContext::app_init_context(
        entry,
        new_task_res.ustack_top(),
        kernel_token(),
        new_task.kstack.get_top(),
        linker_symbol_addr!(trap_handler),
    );
    new_task_trap_cx.x[10] = arg;
    new_task_tid as isize
}

pub fn sys_gettid() -> isize {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .tid as isize
}

/// Reap one exited thread: its exit code goes through `exit_code_ptr` and the
/// tid is returned, so a negative exit code can never be read as an errno.
pub fn sys_waittid(tid: usize, exit_code_ptr: *mut i32) -> isize {
    let task = current_task().unwrap();
    let process = task.process.upgrade().unwrap();
    let task_inner = task.inner_exclusive_access();
    let mut process_inner = process.inner_exclusive_access();
    // a thread cannot wait for itself
    if task_inner.res.as_ref().unwrap().tid == tid {
        return EINVAL;
    }
    let Some(waited_task) = process_inner.tasks.get(tid).and_then(|task| task.as_ref()) else {
        return ESRCH;
    };
    let Some(exit_code) = waited_task.inner_exclusive_access().exit_code else {
        return EAGAIN;
    };
    if let Err(e) = copy_to_user(
        process_inner.memory_set.token(),
        exit_code_ptr as *mut u8,
        &exit_code.to_ne_bytes(),
    ) {
        return e;
    }
    // dealloc the exited thread
    process_inner.tasks[tid] = None;
    tid as isize
}
